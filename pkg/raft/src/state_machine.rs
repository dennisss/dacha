use crate::protos::*;
use common::bytes::Bytes;
use common::errors::*;

// Also good to know is when the last config snapshot happened
//

/*
    Implementing discard in the segmented log mode:
    -> We will see a special 'discard'

*/

#[async_trait]
pub trait StateMachine<R> {
    // TODO: Should probably have a check operation that validates an operation is
    // good before a leader decide to commit them (either way we will still be
    // consistent )

    // ^ issue being that because operations are not independent, this would need to
    // be checked per operation So the alternative would be to require the
    // StateMachine to implement an apply, revert, and commit

    /// Should apply the given operation to the state machine immediately
    /// integrating it
    /// If successful, then some result type can be output that is persisted to
    /// disk but is made available to the task that proposed this change to
    /// receive feedback on how the operation performed
    async fn apply(&self, index: LogIndex, op: &[u8]) -> Result<R>;

    /// Should retrieve the last created snapshot if any is available
    /// This should be a cheap operation that can quickly queried to check on
    /// the last snapshot
    async fn snapshot(&self) -> Option<StateMachineSnapshot>;

    async fn restore(&self, data: Bytes) -> Result<()>;

    // Triggers a new snapshot to begin being created and persisted to disk
    // The index of the last entry applied to the state machine is given as an
    // argument to be stored alongside the snapshot Returns a receiver which
    // resolves once the snapshot has been created or has failed to be created
    // NOTE: Snapshotting should be sufficiently robust that old data is not lost on
    // failed snapshots fn perform_snapshot(&self, last_applied: u64) ->
    // Result<oneshot::Receiver<()>>;

    // simple method
    // We will trigger the matching process during the matching process
    /*
        From the matching process, acquire a snapshot and freeze the log
        - As soon as the log offset has passed, we will be able to commit all of the records properly

        - We will not implement truncate of records
        -  We will only implement truncation and appending after that fac


        RecordIO format:
        - Starts with one super block
            - Specifies the index of the previous log entry
            - Then the rest of the entries are sequential entries in the log
            - Appending a new entry with a lower commit index will be the method of handling truncatations
            -

    */
}

/*
    A Read interface will not work

    -> General idea:
        -> While reading a snapshot, can't snapshot?
        -> Yes, I can snapshot in the case of

    If we use a single file for snapshots:
        ->

    -> To do a file read or write, we need to acquire a write-lock on the file

    -> suppose

    -> Not all data may fit in memory
        - There do exists files on disk such that if we copy those files, it will result in us having a set of

*/

pub struct StateMachineSnapshot {
    /// Index of the last log entry in this snapshot (same value originally
    /// given to the perform_snapshot that created this snapshot )
    pub last_applied: LogIndex,

    /// Number of bytes needed to store this snapshot
    pub size: u64,

    /// A reader for retrieving the contents of the snapshot
    /// TODO: Eventually we need some better way of doing this
    pub data: Bytes,
}
