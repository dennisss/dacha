use executor::child_task::ChildTask;
use net::backoff::*;

use crate::node::blob_store::*;
use crate::node::resources::*;
use crate::proto::*;

pub(super) struct Worker {
    /// Spec that was used to start this worker.
    pub spec: WorkerSpec,

    pub revision: u64,

    /// Id of the most recent container running this worker.
    pub container_id: Option<String>,

    pub state: WorkerState,

    pub start_backoff: ExponentialBackoff,

    /// The worker was recently created or updated so we are waiting for the
    /// worker to be started using the latest WorkerSpec.
    ///
    /// Will be reset to false once we have entired the Starting|Running state.
    pub pending_update: Option<StartWorkerRequest>,

    /// If true, this container won't be restarted regardless of its restart
    /// policy.
    pub permanent_stop: bool,

    /// Leases for all blobs in use by this worker when running.
    /// This is set when transitioning to the Running state and cleared when
    /// entering the Terminal state.
    pub blob_leases: Vec<BlobLease>,
}

pub(super) enum WorkerState {
    /// The worker hasn't yet been scheduled to start running.
    ///
    /// This may be because the worker was only just recently added or it is
    /// missing some resources/dependencies needed to run. By default, if there
    /// are missing resources, we will wait until the resources become available
    /// and then start running the worker.
    ///
    /// TODO: In this state, enumerate all missing requirements
    Pending {
        /// Partial set of requirements needed by this worker which aren't
        /// currently available.
        missing_requirements: ResourceSet,
    },

    /// In this state, we have a running container for this worker.
    Running,

    /// In this state, we already sent a SIGINT to the worker and are waiting
    /// for it to stop on its own.
    ///
    /// If the worker doesn't stop by itself after a timeout, we will transition
    /// to ForceStopping.
    Stopping {
        timer_id: usize,
        timeout_task: ChildTask,
    },

    /// We were just in the Stopping state and sent a SIGKILL to the container
    /// because it was taking too long to stop.
    /// We are currently waiting for the container runtime to report that the
    /// container is completely dead.
    ForceStopping,

    /// The worker's container is dead and we are waiting a reasonable amount of
    /// time before retrying.
    RestartBackoff {
        timer_id: usize,
        timeout_task: ChildTask,
    },

    /// The container has exited and there is no plan to restart it.
    ///
    /// TODO: How do we determine if the
    Done, /*  {
           *     state: WorkerTerminalState
           * } */
}

pub(super) enum WorkerDoneState {
    /// This was a one-off worker (with restart_policy set to something other
    /// than ALWAYS|UNKNOWN) and it completed with a successful exit code
    /// (of 0).
    Successful,

    /// This worker was stopped before it completed its intended number of
    /// attempts.
    ///
    /// - If a worker is killed gracefully with a signal like SIGINT but exits
    ///   with a code of 0, this is considered an Abort instead of a Success.
    /// - If a worker had to be force killed because it was not responding, it
    ///   is considered a failure and will have a Failed terminal state.
    Aborted,

    Failed,
}
