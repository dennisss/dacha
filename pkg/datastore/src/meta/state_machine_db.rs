use core::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::cancellation::{
    AlreadyCancelledToken, CancellationToken, EitherCancelledToken, TriggerableCancellationToken,
};
use executor::child_task::ChildTask;
use executor::lock_async;
use executor::sync::{AsyncMutex, AsyncRwLockReadGuard};
use executor::sync::{AsyncRwLock, PoisonError};
use executor_multitask::{
    CancellationTokenSet, ServiceResource, ServiceResourceReportTracker, ServiceResourceSubscriber,
};
use sstable::EmbeddedDB;

/// Wrapper around the EmbeddedDB instance used in the state machine which
/// allows for swapping the database instance.
pub struct EmbeddedDBStateMachineDatabase {
    state: AsyncRwLock<State>,
    report: Arc<ServiceResourceReportTracker>,
    cancellation_tokens: Arc<CancellationTokenSet>,
}

struct State {
    db: EmbeddedDB,
    watcher: Option<ChildTask>,
    db_canceller: Arc<TriggerableCancellationToken>,
}

#[async_trait]
impl ServiceResource for EmbeddedDBStateMachineDatabase {
    async fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        self.cancellation_tokens.add_cancellation_token(token).await
    }

    async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber> {
        self.report.subscribe()
    }
}

impl EmbeddedDBStateMachineDatabase {
    pub async fn create(db: EmbeddedDB) -> Self {
        let cancellation_tokens = Arc::new(CancellationTokenSet::default());

        // TODO: Dedup this.
        let db_canceller = Arc::new(TriggerableCancellationToken::default());
        db.add_cancellation_token(Arc::new(EitherCancelledToken::new(
            db_canceller.clone(),
            cancellation_tokens.clone(),
        )))
        .await;

        let mut sub = db.new_resource_subscriber().await;

        let initial_report = sub.value().await;
        let report = Arc::new(ServiceResourceReportTracker::new(initial_report));

        let watcher = ChildTask::spawn(Self::subscriber_thread(sub, report.clone()));

        Self {
            state: AsyncRwLock::new(State {
                db,
                watcher: Some(watcher),
                db_canceller,
            }),
            report,
            cancellation_tokens,
        }
    }

    async fn subscriber_thread(
        mut sub: Box<dyn ServiceResourceSubscriber>,
        report: Arc<ServiceResourceReportTracker>,
    ) {
        loop {
            sub.wait_for_change().await;
            report.update(sub.value().await).await;
        }
    }

    pub async fn read<'a>(
        &'a self,
    ) -> Result<EmbeddedDBStateMachineDatabaseReadLock<'a>, PoisonError> {
        Ok(EmbeddedDBStateMachineDatabaseReadLock {
            inner: self.state.read().await?,
        })
    }

    /// NOT CANCEL SAFE
    pub async fn swap_with(&self, new_db: EmbeddedDB) -> Result<()> {
        let mut guard = self.state.write().await?.enter();

        // Create a new state consisting of the new db that we want to swap in.
        let mut state = {
            let db_canceller = Arc::new(TriggerableCancellationToken::default());
            new_db
                .add_cancellation_token(Arc::new(EitherCancelledToken::new(
                    db_canceller.clone(),
                    self.cancellation_tokens.clone(),
                )))
                .await;

            State {
                db: new_db,
                watcher: None, // Started later.
                db_canceller,
            }
        };

        core::mem::swap(&mut state, &mut *guard);

        // 'state' is now the old db state
        // 'guard' is now the new db state.

        // Wait for the old subscriber_thread to finish.
        state.watcher.take().unwrap().cancel().await;

        // Set up a new subscriber for the new database.
        let mut sub = guard.db.new_resource_subscriber().await;
        let initial_report = sub.value().await;
        self.report.update(initial_report).await;
        guard.watcher = Some(ChildTask::spawn(Self::subscriber_thread(
            sub,
            self.report.clone(),
        )));

        guard.exit();

        // Wait for the old database to finish running.
        state.db_canceller.trigger().await;
        state.db.wait_for_termination().await?;

        Ok(())
    }
}

pub struct EmbeddedDBStateMachineDatabaseReadLock<'a> {
    inner: AsyncRwLockReadGuard<'a, State>,
}

impl<'a> Deref for EmbeddedDBStateMachineDatabaseReadLock<'a> {
    type Target = EmbeddedDB;

    fn deref(&self) -> &Self::Target {
        &self.inner.db
    }
}
