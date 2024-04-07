use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use common::bytes::Bytes;
use common::const_default::ConstDefault;
use common::errors::*;
use datastore_meta_client::key_utils::*;
use executor::sync::AsyncMutex;
use executor::{channel, lock, lock_async};
use raft::proto::LogEntryData;
use raft::ReadIndex;
use raft::{proto::LogPosition, proto::Term, LogIndex, PendingExecutionResult};
use sstable::db::{Snapshot, SnapshotIteratorOptions, WriteBatch};
use sstable::iterable::Iterable;

use crate::meta::key_ranges::KeyRanges;
use crate::meta::state_machine::EmbeddedDBStateMachine;
use crate::meta::table_key::*;
use crate::proto::*;

const MAX_KEYS_PER_TRANSACTION: usize = 100;

/// Executes and tracks multi-key transactions being executed on the key value
/// store. This enables parallel execution of transactions with non-conflicting
/// effects.
///
/// The TransactionManager runs on the leader of the Raft group and will error
/// out if that is not the case. For correctness, we require that the all writes
/// since the start of this leader's term go through the TransactionManager.
///
/// A transaction is executed as follows:
///
/// 1. Acquire in-memory term scoped locks for all read/modified key ranges.
///
/// 2. Must minimally (using begin_read(optimistic=true)):
///   - Verify that the leader is done committing any log entries that were
///     added before the start of the leader's current term.
///   - Using the current commit_index on the leader, wait for the state machine
///     to reach at least this point and snapshot it.
///
/// 3. Check that our in-memory locks are still valid (term is the same as the
/// read index).
///
/// 4. Verify that none of the reads in the transaction have changed since the
/// read_index of the transaction.
///   - Can be done with the snapshot in step 2 as we guarantee that there will
///     be no more parallel writes to these keys that aren't already committed
///     because we hold in-memory locks (on the leader who is the only one that
///     can modify them) to prevent that.
///
/// 5. Execute the mutation on the Raft module (passing in the read index from
/// #2 to guarantee we are still the leader).
///
/// 6. Wait for the mutation to fully commit (or get overriden by something else
/// at the same index).
///
/// 7. Release our locks (if they haven't been released already due to the term
/// changing).
///   - Note that these must be released before returning to the user to ensure
///     that
pub struct TransactionManager {
    state: Arc<AsyncMutex<TransactionManagerState>>,
}

struct TransactionManagerState {
    term: Term,

    /// Writes that have been appended to our local log but haven't been
    /// comitted yet. This is a map from internal key to the log index at
    /// which will be comitted.
    locks: KeyRanges<TransactionLock>,
}

#[derive(Clone, Default)]
struct TransactionLock {
    /// Number of TransactionLockHolder instances that exist and are using this
    /// lock.
    num_references: usize,

    mode: TransactionLockMode,

    /// List of thread callbacks which are waiting for this lock to be dropped.
    /// TODO: Limit the max length of this.
    ///
    /// TODO: Switch this to use an Arc<> like mechanism for tracking references
    /// to this.
    waiters: Vec<channel::Sender<()>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransactionLockMode {
    None,
    Read,
    Write,
    ReadWrite,
}

impl Default for TransactionLockMode {
    fn default() -> Self {
        Self::None
    }
}

///
struct TransactionLockHolder<'a> {
    term: Term,
    locks: KeyRanges<TransactionLockMode>,
    manager_state: &'a AsyncMutex<TransactionManagerState>,
}

impl<'a> TransactionLockHolder<'a> {
    /// NOT CANCEL SAFE
    async fn release(mut self) {
        let mut state = match self.manager_state.lock().await {
            Ok(v) => v.enter(),
            // If poisoned, then there is nothing to free.
            Err(_) => return,
        };

        if state.term != self.term {
            state.exit();
            return;
        }

        for item in self.locks.iter() {
            state
                .locks
                .range(item.start_key.clone(), item.end_key.clone(), |lock| {
                    lock.num_references -= 1;
                    lock.num_references > 0
                });
        }

        state.exit();
    }
}

impl TransactionManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(AsyncMutex::new(TransactionManagerState {
                term: Term::DEFAULT,
                locks: KeyRanges::new(),
            })),
        }
    }

    /// CANCEL SAFE
    pub async fn execute(
        &self,
        transaction: Transaction,
        node: Arc<raft::Node<()>>,
        state_machine: Arc<EmbeddedDBStateMachine>,
    ) -> Result<LogIndex> {
        // Spawn in a detached task to make the outer function cancel safe.
        executor::spawn(Self::execute_inner(
            self.state.clone(),
            transaction,
            node,
            state_machine,
        ))
        .join()
        .await
    }

    /// Actual implementation of execute(). Must not be interrupted during
    /// execution.
    ///
    /// NOT CANCEL SAFE
    async fn execute_inner(
        manager_state: Arc<AsyncMutex<TransactionManagerState>>,
        transaction: Transaction,
        node: Arc<raft::Node<()>>,
        state_machine: Arc<EmbeddedDBStateMachine>,
    ) -> Result<LogIndex> {
        let required_locks = Self::get_required_locks(&transaction)?;
        let write_batch = Self::create_write_batch(&transaction)?;

        // TODO: Automatically rewrite non-permanent errors.

        // TODO: Limit the number of keys in a request
        // Ideally <1000

        let term = node.server().currently_leader().await?;

        // Step 1
        let lock_result = lock!(state <= manager_state.lock().await?, {
            Self::acquire_locks(&manager_state, &mut *state, term, required_locks)
        });

        let lock_holder = match lock_result {
            Ok(v) => v,
            Err(conflict) => {
                let _ = conflict.recv().await;
                return Err(rpc::Status::failed_precondition(
                    "Conflict while acquiring locks. Please retry",
                )
                .into());
            }
        };

        let result =
            Self::execute_with_locks(transaction, node, state_machine, &lock_holder, write_batch)
                .await;

        // Step 7
        lock_holder.release().await;

        result
    }

    /// Performs the execution while holding locks on all keys involved.
    async fn execute_with_locks(
        transaction: Transaction,
        node: Arc<raft::Node<()>>,
        state_machine: Arc<EmbeddedDBStateMachine>,
        lock_holder: &TransactionLockHolder<'_>,
        mut write_batch: WriteBatch,
    ) -> Result<LogIndex> {
        // Step 2
        let read_index = node.server().begin_read(true).await?;

        // Step 3
        if read_index.term() != lock_holder.term {
            return Err(rpc::Status::failed_precondition(
                "Locks lost due to leadership change since transaction start.",
            )
            .into());
        }

        // Step 4
        if !transaction.reads().is_empty() {
            // Read against the latest version of the database.
            let snapshot = state_machine.snapshot().await;
            if !Self::verify_reads(&transaction, &snapshot, read_index.index()).await? {
                return Err(rpc::Status::failed_precondition(
                    "Changes have occured since the read index",
                )
                .into());
            }
        }

        // Add the transaction time.
        // NOTE: We don't currently make gurantees that transaction times are monotonic
        // and it may be much earlier than the time at which the transaction is actually
        // committed if the system is in the middle of a network partition.
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        write_batch.put(&TableKey::transaction_time(), &time.to_le_bytes());

        let mut entry = LogEntryData::default();
        entry.set_command(write_batch.as_bytes());

        // Step 5
        let pending_execution = node
            .server()
            .execute_after_read(entry, Some(read_index))
            .await?;

        // Step 6
        //
        // TODO: This will wait for the change to also be applied to the state
        // machine which is not strictly needed.
        let commited_index = match pending_execution.wait().await {
            PendingExecutionResult::Committed { log_index, .. } => log_index,
            PendingExecutionResult::Cancelled => {
                return Err(rpc::Status::failed_precondition("message").into());
            }
        };

        Ok(commited_index)
    }

    /// Derives the set of all locks needed to execute the given transaction.
    fn get_required_locks(transaction: &Transaction) -> Result<KeyRanges<TransactionLockMode>> {
        let mut required_locks = KeyRanges::new();

        for range in transaction.reads() {
            required_locks.range(range.start_key(), range.end_key(), |mode| {
                *mode = TransactionLockMode::Read;
                true
            });
        }

        let mut has_overlapping_writes = false;
        for op in transaction.writes() {
            let (start_key, end_key) = single_key_range(op.key());

            required_locks.range(start_key, end_key, |mode| {
                *mode = match mode {
                    TransactionLockMode::Read => TransactionLockMode::ReadWrite,
                    TransactionLockMode::None => TransactionLockMode::Write,
                    TransactionLockMode::Write | TransactionLockMode::ReadWrite => {
                        has_overlapping_writes = true;
                        *mode
                    }
                };

                true
            });

            if has_overlapping_writes {
                for range in required_locks.iter() {
                    println!("RANGE: {:?} {:?}", range.start_key, range.end_key);
                }

                return Err(
                    rpc::Status::invalid_argument("Transaction has overlapping writes").into(),
                );
            }
        }

        Ok(required_locks)
    }

    fn create_write_batch(transaction: &Transaction) -> Result<WriteBatch> {
        let mut write = WriteBatch::new();

        for op in transaction.writes() {
            match op.typ_case() {
                OperationTypeCase::Put(value) => {
                    write.put(op.key(), value);
                }
                OperationTypeCase::Delete(_) => {
                    write.delete(op.key());
                }
                OperationTypeCase::NOT_SET => {
                    return Err(rpc::Status::invalid_argument("Invalid operation").into());
                }
            }
        }

        Ok(write)
    }

    /// Atomically acquires all of the requested locks.
    /// In other words locking is all or nothing.
    ///
    /// NOT CANCEL SAFE
    ///
    /// Returns: None iff we were able to acquire all locks. Otherwise Some
    /// channel which will be closed once the first conflict is resolved.
    ///
    /// TODO: Return a RAII object to monitor the eventual return of the locks
    /// (under the same term).
    #[must_use]
    fn acquire_locks<'a>(
        state_ref: &'a AsyncMutex<TransactionManagerState>,
        state: &mut TransactionManagerState,
        term: Term,
        locks: KeyRanges<TransactionLockMode>,
    ) -> Result<TransactionLockHolder<'a>, channel::Receiver<()>> {
        if term > state.term {
            state.locks.clear();
            state.term = term.clone();
        } else if term < state.term {
            // Means that we immediately lost leadership after starting the transaction.
            return Err(channel::bounded(1).1);
        }

        let mut conflict = None;
        let mut num_locked = 0;
        for item in locks.iter() {
            state
                .locks
                .range(item.start_key.clone(), item.end_key.clone(), |lock| {
                    if conflict.is_some() {
                        return lock.num_references > 0;
                    }

                    let good = match *item.value {
                        TransactionLockMode::None | TransactionLockMode::ReadWrite => {
                            lock.mode == TransactionLockMode::None
                        }
                        TransactionLockMode::Read => {
                            lock.mode == TransactionLockMode::None
                                || lock.mode == TransactionLockMode::Read
                        }
                        TransactionLockMode::Write => {
                            lock.mode == TransactionLockMode::None
                                || lock.mode == TransactionLockMode::Write
                        }
                    };

                    if good {
                        num_locked += 1;
                        lock.num_references += 1;
                    } else {
                        let (sender, receiver) = channel::bounded(1);
                        lock.waiters.push(sender);
                        conflict = Some(receiver);
                    }

                    lock.num_references > 0
                });

            if conflict.is_some() {
                break;
            }
        }

        // Rollback if we failed
        if conflict.is_some() {
            for item in locks.iter() {
                state
                    .locks
                    .range(item.start_key.clone(), item.end_key.clone(), |lock| {
                        if num_locked > 0 {
                            lock.num_references -= 1;
                            num_locked -= 1;
                        }

                        lock.num_references > 0
                    });

                if num_locked == 0 {
                    break;
                }
            }
        }

        if let Some(conflict) = conflict {
            return Err(conflict);
        }

        Ok(TransactionLockHolder {
            term,
            locks,
            manager_state: state_ref,
        })
    }

    /// CANCEL SAFE
    #[must_use]
    async fn verify_reads(
        transaction: &Transaction,
        snapshot: &Snapshot,
        latest_commited_index: LogIndex,
    ) -> Result<bool> {
        if transaction.read_index() < snapshot.compaction_waterline().unwrap() {
            return Err(
                rpc::Status::failed_precondition("Transaction's read_index is too old.").into(),
            );
        }

        // TODO: Change the iterator to have a lower bound on the sequence (as that way
        // we can skip reaidng from disk).
        if transaction.read_index() > latest_commited_index.value() {
            return Err(
                rpc::Status::invalid_argument("Transaction read_index is in the future").into(),
            );
        }

        if transaction.read_index() == snapshot.last_sequence() {
            return Ok(true);
        }

        let mut iter_options = SnapshotIteratorOptions::default();
        iter_options.first_sequence = Some(transaction.read_index() + 1);

        let mut iter = snapshot.iter_with_options(iter_options).await?;

        for range in transaction.reads() {
            iter.seek(range.start_key()).await?;

            while let Some(entry) = iter.next().await? {
                if &entry.key >= range.end_key() {
                    break;
                }

                // TODO: We don't actually need to check as we used the iter_options to only
                // return values where this is true.
                if entry.sequence > transaction.read_index() {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}
