use core::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::cancellation::{AlreadyCancelledToken, CancellationToken};
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
    db: AsyncRwLock<(EmbeddedDB, Option<ChildTask>)>,
    report: Arc<ServiceResourceReportTracker>,
    cancellation_tokens: Arc<CancellationTokenSet>,
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
        db.add_cancellation_token(cancellation_tokens.clone()).await;

        let mut sub = db.new_resource_subscriber().await;

        let initial_report = sub.value().await;
        let report = Arc::new(ServiceResourceReportTracker::new(initial_report));

        let watcher = ChildTask::spawn(Self::subscriber_thread(sub, report.clone()));

        Self {
            db: AsyncRwLock::new((db, Some(watcher))),
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
            inner: self.db.read().await?,
        })
    }

    /// NOT CANCEL SAFE
    pub async fn swap_with(&self, mut new_db: EmbeddedDB) -> Result<()> {
        let mut guard = self.db.write().await?.enter();

        let mut tuple = (new_db, None);
        core::mem::swap(&mut tuple, &mut *guard);

        // Wait for the old subscriber_thread to finish.
        tuple.1.take().unwrap().cancel().await;

        // Set up a new subscriber for the new database.
        let mut sub = guard.0.new_resource_subscriber().await;
        let initial_report = sub.value().await;
        self.report.update(initial_report).await;
        guard.1 = Some(ChildTask::spawn(Self::subscriber_thread(
            sub,
            self.report.clone(),
        )));

        guard.exit();

        // Wait for the old database to finish running.
        tuple
            .0
            .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
            .await;
        tuple.0.wait_for_termination().await?;

        Ok(())
    }
}

pub struct EmbeddedDBStateMachineDatabaseReadLock<'a> {
    inner: AsyncRwLockReadGuard<'a, (EmbeddedDB, Option<ChildTask>)>,
}

impl<'a> Deref for EmbeddedDBStateMachineDatabaseReadLock<'a> {
    type Target = EmbeddedDB;

    fn deref(&self) -> &Self::Target {
        &self.inner.0
    }
}
