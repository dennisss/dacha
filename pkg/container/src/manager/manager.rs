use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use common::errors::*;
use crypto::random::{SharedRng, SharedRngExt};
use datastore::meta::client::MetastoreClient;
use datastore::meta::client::MetastoreClientInterface;
use datastore::meta::client::MetastoreTransaction;
use protobuf::Message;
use rpc_util::{AddReflection, NamedPortArg};

use crate::meta::GetClusterMetaTable;
use crate::proto::*;

/*
When a manager test starts up, it will
- Acquire a metastore lock under `/system/manager/lock`
  - If it can't it will sleep for 30 seconds and try again.
- Enumerate all Job instances in the database.
  - For each job instance, verify that they there are workers for each job assigned to nodes.
- Finally, loop through each Node and ensure that it has all required nodes.
  ^ After the initial

- Want to have an active connection to each node to receive change updates.


Manager Role:
- Keep the metadata store alive
- Ping the nodes and see that they have the
- Ensure that every job has all its workers to some node
    - If a node is dead, we may want to move all of its workers to another node (assuming they
      are moveable).
- Ensure that every blob has at least N replicas.
- Delete blobs that are not in use for at least N days.

*/

/*
Threads:
- RPC server

// - Change actuator.
//     - Listened to events:
//         - NewJob
//         - NewBlob
- Node poller
    - Tries to contact all nodes in the cluster.
    - Verifies they are running the right workers.
    - When workers become ready, the manager will mark them as ready/not-ready in the metadata store.
        -> Issue is that this is fragile?
    - TODO: Replace with just having the node watch for updates?

Should notds support pulling blobs from our servers?
- Yes because that is more efficient.

*/

// TODO: Node ids should be randomly generated once and we should only attempt
// to create a NodeMetadata once.

regexp!(JOB_NAME_PATTERN => "^((?:[a-z](?:[a-z0-9\\-_]*[a-z0-9])?)\\.?)+$");

/// The max length of a URL is 255 characters.
/// It's somewhat difficult to verify that the name won't cause an overflow in
/// all contexts, so just to be safe, we won't allow jobs with names close to
/// that limit (minus a buffer for DNS names, worker ids, etc.)
const JOB_NAME_MAX_SIZE: usize = 180;

const JOB_NAME_MAX_LABEL_LENGTH: usize = 63;

/// Interval at which the manager will re-check the state of all jobs in the
/// cluster to ensure that all have all workers assigned to healthy nodes.
const JOB_RECONCILE_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// Maximum fraction of nodes which are allowed to be dead while we are evicting
/// workers from dead nodes.
///
/// This is meant to be a small fraction of nodes in order to protect the
/// cluster from having a small fraction of nodes suddenly assigned to perform
/// all the work of the cluster because of network partitions providing access
/// to the metastore.
const NODE_MAX_DEAD_FRACTION_FOR_EVICTION: f32 = 0.3;

/*
TODO: We need to check that the node last_seen timeout is much longer than it takes for the metastore to fail over and for the node to retry.
*/

/// NOTE: Cloning a 'Manager' instance will reference the same internal object.
#[derive(Clone)]
pub struct Manager {
    meta_client: Arc<dyn MetastoreClientInterface>,
    rng: Arc<dyn SharedRng>,
}

impl Manager {
    pub fn new(meta_client: Arc<dyn MetastoreClientInterface>, rng: Arc<dyn SharedRng>) -> Self {
        Self { meta_client, rng }
    }

    /// Entrypoint of the background manager thread which periodically ensures
    /// that the cluster is in a good state.
    pub async fn run(self) -> Result<()> {
        // TODO: Require holding a metastore lock to running this loop (mainly to avoid
        // contention).
        loop {
            self.run_once().await?;
            executor::sleep(JOB_RECONCILE_RETRY_INTERVAL).await;
        }

        Ok(())
    }

    async fn run_once(&self) -> Result<()> {
        let mut jobs = self
            .meta_client
            .cluster_table::<JobMetadata>()
            .list()
            .await?;
        for job in jobs {
            if let Err(e) = self.reconcile_job(job.spec().name()).await {
                eprintln!("Failed to reconcile job {}: {}", job.spec().name(), e);
            }
        }

        Ok(())
    }

    fn is_valid_job_name(name: &str) -> bool {
        if name.len() > JOB_NAME_MAX_SIZE {
            return false;
        }

        if !JOB_NAME_PATTERN.test(name) {
            return false;
        }

        for label in name.split('.') {
            if label.len() > JOB_NAME_MAX_LABEL_LENGTH {
                return false;
            }
        }

        if name.ends_with(".") {
            return false;
        }

        true
    }

    /// Implementation of the StartJob RPC handler which creates new jobs in the
    /// cluster upon request from the user.
    async fn start_job_impl(&self, request: &StartJobRequest) -> Result<()> {
        // Sanity check that the job is probably startable and doesn't contain any
        // invalid internal fields.
        {
            let spec: &JobSpec = request.spec();
            if spec.replicas() == 0 {
                return Err(rpc::Status::invalid_argument(
                    "Job not allowed to have zero replicas.",
                )
                .into());
            }

            if !spec.worker().name().is_empty() {
                return Err(
                    rpc::Status::invalid_argument("Not allowed to specify a worker name").into(),
                );
            }

            if !Self::is_valid_job_name(spec.name()) {
                return Err(rpc::Status::invalid_argument("Invalid job name").into());
            }

            for port in spec.worker().ports() {
                if port.number() != 0 {
                    return Err(rpc::Status::invalid_argument(
                        "Not allowed to specify port numbers",
                    )
                    .into());
                }

                if port.typ() == PortType::UNKNOWN {
                    return Err(rpc::Status::invalid_argument("No port type specified").into());
                }

                if port.typ() != PortType::TCP {
                    return Err(rpc::Status::invalid_argument(
                        "Only TCP ports are currently supported",
                    )
                    .into());
                }

                if port.protocol().is_empty() {
                    return Err(
                        rpc::Status::invalid_argument("No port protocol(s) specified").into(),
                    );
                }
            }

            // TODO: Require authentication to create system services.
            if spec.worker().persistent() && !spec.name().starts_with("system.") {
                return Err(rpc::Status::invalid_argument(
                    "Not allowed to specify persistent worker flag.",
                )
                .into());
            }

            // TODO: Check no build rules still present in volumes.
        }

        run_transaction!(&self.meta_client, txn, {
            self.start_job_transaction(request, &txn).await?;
        });

        // TODO: Make this optionally syncronous.
        // Currently this needs to be syncronous so that the bootstrapping command
        // works.
        // maybe have a wait_for_
        self.reconcile_job(request.spec().name()).await?;

        // Trigger re-calculation of the workers.
        // - Look up the job
        // - Look up all workers associated with the job (ideally transactionally).
        // - If we need more workers, look up all nodes and try to find one .
        // -

        // Thread 1: React to changes in individual jobs. Re-calculate requirements.
        // - If we need to

        // /cluster/worker/[worker_name]
        // /cluster/worker_by_node/[node_id]

        // For each node, we do want to track:
        // - Assigned resources
        // - Assigned worker names.

        Ok(())
    }

    /// In a single metastore transaction, this adds a job to the cluster.
    async fn start_job_transaction(
        &self,
        request: &StartJobRequest,
        txn: &MetastoreTransaction<'_>,
    ) -> Result<()> {
        let job_table = txn.cluster_table::<JobMetadata>();

        let existing_job = job_table.get(request.spec().name()).await?;

        if existing_job.is_none() {
            // A job can only be created if there are no job whose name is a prefix of the
            // new job name (at segment boundaries).
            //
            // TODO: First check whether or not the job just plain old exists.
            //
            // In other words, for every '[job_name_i]' in the cluster already,
            // '[job_name_i].' must not be a prefix of '[new_job_name].'
            let name_segments = request.spec().name().split('.').collect::<Vec<&str>>();
            for i in 1..name_segments.len() - 1 {
                let prefix = name_segments[0..i].join(".");
                if let Some(_) = txn.cluster_table::<JobMetadata>().get(&prefix).await? {
                    return Err(rpc::Status::invalid_argument(format!(
                        "A job already exists with a prefix with a new job name: {}",
                        prefix
                    ))
                    .into());
                }
            }
        }

        let mut job_meta = existing_job.unwrap_or_else(|| JobMetadata::default());

        if job_meta.spec().worker() != request.spec().worker() {
            job_meta.set_worker_revision(txn.read_index().await);
        }

        job_meta.set_spec(request.spec().clone());

        job_table.put(&job_meta).await?;

        Ok(())
    }

    async fn reconcile_job(&self, job_name: &str) -> Result<()> {
        let txn = self.meta_client.new_transaction().await?;

        let nodes_table = txn.cluster_table::<NodeMetadata>();
        let jobs_table = txn.cluster_table::<JobMetadata>();
        let workers_table = txn.cluster_table::<WorkerMetadata>();

        let job = jobs_table
            .get(job_name)
            .await?
            .ok_or_else(|| err_msg("Job doesn't exist"))?;

        // TODO: This read operation will cause a lot of contention as nodes may be
        // simultaneously updating their status.
        let mut nodes = nodes_table.list().await?;
        if nodes.is_empty() {
            // TODO: This may be problematic during initial bootstrapping of the cluster.
            return Err(err_msg("No nodes present"));
        }

        // Mapping from node id to the index of the NodeMetadata in 'nodes'.
        let mut nodes_by_id = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            nodes_by_id.insert(node.id(), i);
        }

        let mut existing_workers = workers_table.list_by_job(job_name).await?;

        // TODO: Do not re-schedule drained workers if using distinct_nodes on the same
        // node until it is done being cleaned up.
        let mut drained_workers = existing_workers
            .drain_filter(|worker| worker.drain())
            .collect::<Vec<_>>();

        existing_workers.retain(|worker| !worker.drain());

        // Indexes of all nodes (in the 'nodes' vector) which we will consider for
        // running workers in this job.
        let mut remaining_nodes = vec![];
        for i in 0..nodes.len() {
            remaining_nodes.push(i);
        }

        // TODO: Filter out any nodes which are not healthy.

        if job.spec().scheduling().specific_nodes_len() > 0 {
            remaining_nodes.retain(|i| {
                let current_id = nodes[*i].id();
                job.spec()
                    .scheduling()
                    .specific_nodes()
                    .iter()
                    .find(|id| **id == current_id)
                    .is_some()
            });

            if remaining_nodes.len() != job.spec().scheduling().specific_nodes_len() {
                return Err(err_msg("Some nodes in specific_nodes weren't found"));
            }
        }

        // TODO: Need to increment ref counts to blobs.
        // ^ Yes.

        let mut update_incomplete = false;

        // Old workers associated with this job which we ended up not being able to
        // re-use.
        let mut old_workers = vec![];

        /*
        If a node dies, we don't know if it will ever come back.
        - In general, nodes should continue working with as few dependencies as possible (until they die)
        - Once not seem for more than 30 seconds, all workers on a node will be evicted and moved elsewhere
            - If some services like disk servers depend on disks, then naturally it can't be evicted
            - A network outage may cause a lot of nodes to suddenly become unavailable.

        - Eventually need
        */

        // TODO: Any workers in a DONE state (or a RestartPolicy preventing from than
        // one )

        // TODO: Implement each replica as a separate transaction.
        for i in 0..(job.spec().replicas() as usize) {
            // Attempt to select an existing worker that we want to re-use.
            let existing_worker = {
                let mut picked_worker = None;
                while let Some(worker) = existing_workers.pop() {
                    // The existing worker must still be in our selected node subset to be
                    // re-used.
                    if !remaining_nodes
                        .iter()
                        .find(|idx| nodes[**idx].id() == worker.assigned_node())
                        .is_some()
                    {
                        old_workers.push(worker);
                        continue;
                    }

                    picked_worker = Some(worker);
                    break;
                }

                picked_worker
            };

            let assigned_node_index = {
                if let Some(existing_worker) = &existing_worker {
                    *nodes_by_id
                        .get(&existing_worker.assigned_node())
                        .ok_or_else(|| err_msg("Failed to find assigned node"))?
                } else {
                    // TODO: Don't make this a permanent failure. Instead come back to this job
                    // later once we have more nodes.
                    if remaining_nodes.is_empty() {
                        update_incomplete = true;
                        break;
                    }

                    let selected_idx = self.rng.between::<usize>(0, remaining_nodes.len()).await;
                    remaining_nodes[selected_idx]
                }
            };

            // If we are only allowed to assign to distinct nodes, remove the selected node
            // for the node set for future decisions.
            if job.spec().scheduling().distinct_nodes() {
                remaining_nodes.retain(|idx| *idx != assigned_node_index);
            }

            // Skip if the existing worker is already up to date.
            if let Some(existing_worker) = &existing_worker {
                if existing_worker.revision() == job.worker_revision() {
                    continue;
                }
            }

            let assigned_node = &mut nodes[assigned_node_index];

            let mut new_worker = WorkerMetadata::default();
            new_worker.set_assigned_node(assigned_node.id());

            let new_spec = self
                .create_allocated_worker_spec(
                    job.spec().name(),
                    &job.spec().worker(),
                    existing_worker.as_ref().map(|t| t.spec()),
                    assigned_node,
                )
                .await?;
            new_worker.set_spec(new_spec);
            new_worker.set_revision(job.worker_revision());

            // Update the worker
            // TODO: Skip this if the worker hasn't changed at all.
            workers_table.put(&new_worker).await?;

            // Update the node
            {
                let mut dirty = false;

                let mut old_port_nums = HashSet::new();
                if let Some(existing_worker) = &existing_worker {
                    for port in existing_worker.spec().ports() {
                        old_port_nums.insert(port.number());
                    }
                }

                for port in new_worker.spec().ports() {
                    if !old_port_nums.remove(&port.number()) {
                        assigned_node.allocated_ports_mut().insert(port.number());
                        dirty = true;
                    }
                }

                for old_port in old_port_nums {
                    assigned_node.allocated_ports_mut().remove(&old_port);
                    dirty = true;
                }

                if dirty {
                    nodes_table.put(assigned_node).await?;
                }
            }
        }

        // TODO: If all workers are in DONE state, then we could delete the entire Job
        // (because we probably want some way for someone to later query the state of
        // all past jobs).

        // TODO: We can't delete a worker or switch it to another node until we know
        // that the node to which it was originally assigned has stopped the
        // workers (otherwise we might end up re-assigning resources before they
        // are available?)
        // - There is a similar issue when switch to a new worker spec with conflicting
        //   requirements
        // - This should be solved if the Node is smart enough to handle resources and
        //   can mark workers are Pending before they are schedulable
        // - For ports, we do need to ensure that we check host names to ensure that
        //   users are querying the right worker.

        // Stop all extra instances.
        existing_workers.extend(old_workers.into_iter());
        for mut existing_worker in existing_workers {
            // TODO: Eventually once the node has stopped the worker, we should delete the
            // WorkerMetadata entry for this.
            existing_worker.set_drain(true);
            workers_table.put(&existing_worker).await?;

            let node = &mut nodes[*nodes_by_id
                .get(&existing_worker.assigned_node())
                .ok_or_else(|| err_msg("Failed to find assigned node"))?];

            let mut dirty = false;
            for port in existing_worker.spec().ports() {
                node.allocated_ports_mut().remove(&port.number());
                dirty = true;
            }

            if dirty {
                nodes_table.put(node).await?;
            }
        }

        txn.commit().await?;

        self.cleanup_drained(&drained_workers).await?;

        Ok(())
    }

    /// Given a set of workers that are drained, this will remove the metadata
    /// once the WorkerStateMetadata is marked as DONE (indicating that this
    /// worker will never be started again by the node).
    ///
    /// NOTE: This doesn't need to use a transaction for reading the
    /// WorkerMetadata because it will never transition away from a 'drained'
    /// state.
    ///
    /// TODO: If some workers were cleaned up, we should use this as an
    /// indication that we should try re-reconciling the job (in case this
    /// allows us to schedule more stuff now).
    async fn cleanup_drained(&self, drained_workers: &[WorkerMetadata]) -> Result<()> {
        // NOTE: This transaction is mainly for batching the writes.
        let mut txn = self.meta_client.new_transaction().await?;

        for worker in drained_workers {
            let state_meta = self
                .meta_client
                .cluster_table::<WorkerStateMetadata>()
                .get(worker.spec().name())
                .await?;

            let state_meta = match state_meta {
                Some(v) => v,
                None => continue,
            };

            if state_meta.state() == WorkerStateMetadata_ReportedState::DONE
                && state_meta.worker_revision() == worker.revision()
            {
                txn.cluster_table().delete(worker).await?;
                txn.cluster_table().delete(&state_meta).await?;
            }
        }

        txn.commit().await?;

        Ok(())
    }

    // TODO: We' should avoid allocating the same ports very frequetly. we will also
    // need to validate that clients don't accidentally contact the wrong server by
    // checking the dns name requested (probably doable at the TLS level)

    /// Creates a worker
    ///
    /// TODO: This must mutate the allocated ports set so that we don't end up
    /// obtaining the same port for multiple separate ports.
    async fn create_allocated_worker_spec(
        &self,
        job_name: &str,
        job_worker_spec: &WorkerSpec,
        old_spec: Option<&WorkerSpec>,
        node: &NodeMetadata,
    ) -> Result<WorkerSpec> {
        let mut spec = job_worker_spec.clone();

        let worker_name = if let Some(spec) = &old_spec {
            spec.name().to_string()
        } else {
            // NOTE: We assume that this will generate a unique worker id which has never
            // been seen before but we don't currently validate that the worker
            // doesn't exist yet.
            let mut name = job_name.to_string();
            name.push('.');
            name.push_str(&crate::manager::new_worker_id(self.rng.as_ref()).await);
            name
        };

        spec.set_name(worker_name.as_str());

        // Newly allocated ports. Used to ensure we don't double allocate ports not yet
        // accounted for in the NodeMetadata.
        let mut new_ports = HashSet::new();

        for port in spec.ports_mut() {
            // If updating an existing worker, attempt to re-use existing port assignments.
            if let Some(old_spec) = old_spec.clone() {
                if let Some(old_port) = old_spec.ports().iter().find(|v| v.name() == port.name()) {
                    port.set_number(old_port.number());
                    continue;
                }
            }

            // Otherwise, allocate a new port on the node.

            let mut found_port_num = false;
            for port_num in
                node.allocatable_port_range().start()..node.allocatable_port_range().end()
            {
                if node.allocated_ports().contains(&port_num) || new_ports.contains(&port_num) {
                    continue;
                }

                port.set_number(port_num);
                new_ports.insert(port_num);
                found_port_num = true;
                break;
            }

            if port.number() == 0 {
                return Err(err_msg("Failed to allocate a new port number"));
            }
        }

        for volume in spec.volumes_mut() {
            match volume.source_case() {
                WorkerSpec_VolumeSourceCase::NOT_SET => {}
                WorkerSpec_VolumeSourceCase::Bundle(_) => {}
                WorkerSpec_VolumeSourceCase::PersistentName(name) => {
                    // Persistent volumes should be specific to individual workers.
                    // TODO: Consider moving this local to the node?
                    // (or have a system for making persistent volume claims?)
                    let mut n = worker_name.to_string();
                    n.push('/');
                    n.push_str(name.as_str());

                    volume.set_persistent_name(n);
                }
                WorkerSpec_VolumeSourceCase::BuildTarget(_) => {}
            }
        }

        // TODO: Verify no duplicate volumes?

        Ok(spec)
    }

    async fn allocate_blobs_impl<'a>(
        &self,
        request: rpc::ServerRequest<AllocateBlobsRequest>,
        response: &mut rpc::ServerResponse<'a, AllocateBlobsResponse>,
    ) -> Result<()> {
        // TODO: Filter out unhealthy nodes.
        let mut nodes = self
            .meta_client
            .cluster_table::<NodeMetadata>()
            .list()
            .await?;

        self.rng.shuffle(&mut nodes).await;

        let txn = self.meta_client.new_transaction().await?;
        let blobs_table = txn.cluster_table::<BlobMetadata>();

        for spec in request.blob_specs() {
            // TODO: Validate the blob id format.

            let mut blob = blobs_table.get(spec.id()).await?.unwrap_or_else(|| {
                let mut b = BlobMetadata::default();
                b.set_spec(spec.clone());
                b
            });

            let mut num_uploaded = 0;

            let mut existing_node_ids = HashSet::new();
            for replica in blob.replicas() {
                existing_node_ids.insert(replica.node_id());

                if replica.uploaded() {
                    num_uploaded += 1;
                }
            }

            if num_uploaded > 0 {
                continue;
            }

            while blob.replicas().len() < 1 {
                let mut new_node_id = None;
                for node in &nodes {
                    if !existing_node_ids.contains(&node.id()) {
                        new_node_id = Some(node.id());
                        break;
                    }
                }

                let new_node_id = new_node_id.ok_or_else(|| err_msg("Failed to get a node"))?;

                let mut replica = BlobReplica::default();
                replica.set_node_id(new_node_id);
                replica.set_timestamp(std::time::SystemTime::now());
                blob.add_replicas(replica);
            }

            let mut num_pushing = 0;
            for replica in blob.replicas() {
                if replica.uploaded() {
                    continue;
                }

                if num_pushing == 2 {
                    break;
                }
                num_pushing += 1;

                let mut assignment = BlobAssignment::default();
                assignment.set_blob_id(spec.id());
                assignment.set_node_id(replica.node_id());
                response.value.add_new_assignments(assignment);
            }

            blobs_table.put(&blob).await?;
        }

        txn.commit().await?;

        Ok(())
    }
}

#[async_trait]
impl ManagerService for Manager {
    async fn StartJob(
        &self,
        request: rpc::ServerRequest<StartJobRequest>,
        response: &mut rpc::ServerResponse<StartJobResponse>,
    ) -> Result<()> {
        self.start_job_impl(&request.value).await
    }

    async fn AllocateBlobs(
        &self,
        request: rpc::ServerRequest<AllocateBlobsRequest>,
        response: &mut rpc::ServerResponse<AllocateBlobsResponse>,
    ) -> Result<()> {
        self.allocate_blobs_impl(request, response).await
    }
}

#[cfg(test)]
mod tests {
    use datastore::meta::TestMetastore;
    use protobuf::text::ParseTextProto;

    use super::*;

    #[testcase]
    async fn can_add_a_job() -> Result<()> {
        let rng = Arc::new(crypto::random::ChaCha20RNG::new());
        let meta = TestMetastore::create().await?;

        let meta_client = meta.create_client().await?;

        // TODO: Add a valid last_seen
        let node1 = NodeMetadata::parse_text(
            r#"
            id: 1
            state: ACTIVE
            address: "10.100.0.101:10400"
            allocatable_port_range {
                start: 8000
                end: 9000
            }
        "#,
        )?;

        meta_client
            .cluster_table::<NodeMetadata>()
            .put(&node1)
            .await?;

        let mut request = StartJobRequest::parse_text(
            r#"
            spec {
                name: "adder"
                replicas: 1
                worker {
                    args: ["/bin/sleep", "1000"]
                }
            }
        "#,
        )?;

        let manager = Manager::new(
            Arc::new(meta.create_client().await?),
            Arc::new(crypto::random::ChaCha20RNG::new()), // Fixed seed
        );
        manager.start_job_impl(&request).await?;

        let expected_workers = vec![WorkerMetadata::parse_text(
            r#"
            spec {
                name: "adder.p4rvyfqvb147y"
                args: [
                    "/bin/sleep",
                    "1000"
                ]
            }
            assigned_node: 1
            revision: 3
        "#,
        )?];

        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        // Verify that doing more iterations doesn't change anything.
        manager.run_once().await?;
        manager.run_once().await?;
        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        // Start it again (should be idempotent)
        manager.start_job_impl(&request).await?;

        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        // Make a change.
        request.spec_mut().worker_mut().args_mut()[1] = "2000".into();
        manager.start_job_impl(&request).await?;

        // Will re-use the same name but with a newer revision.
        let expected_workers = vec![WorkerMetadata::parse_text(
            r#"
            spec {
                name: "adder.p4rvyfqvb147y"
                args: [
                    "/bin/sleep",
                    "2000"
                ]
            }
            assigned_node: 1
            revision: 6
        "#,
        )?];
        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        Ok(())
    }

    #[testcase]
    async fn job_on_distinct_nodes() -> Result<()> {
        let rng = Arc::new(crypto::random::ChaCha20RNG::new());
        let meta = TestMetastore::create().await?;

        let meta_client = meta.create_client().await?;

        // TODO: Add a valid last_seen
        let node1 = NodeMetadata::parse_text(
            r#"
            id: 1
            state: ACTIVE
            address: "10.100.0.101:10400"
            allocatable_port_range {
                start: 8000
                end: 9000
            }
        "#,
        )?;

        let mut node2 = node1.clone();
        node2.set_id(2u64);

        meta_client
            .cluster_table::<NodeMetadata>()
            .put(&node1)
            .await?;
        meta_client
            .cluster_table::<NodeMetadata>()
            .put(&node2)
            .await?;

        let mut request = StartJobRequest::parse_text(
            r#"
            spec {
                name: "daemon"
                replicas: 3
                worker { args: ["/bin/stuff"] }
                scheduling {
                    distinct_nodes: true
                }
            }
            "#,
        )?;

        let manager = Manager::new(
            Arc::new(meta.create_client().await?),
            Arc::new(crypto::random::ChaCha20RNG::new()), // Fixed seed
        );
        manager.start_job_impl(&request).await?;

        let expected_workers = vec![
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.nxzzqfbp3eayj"
                    args: ["/bin/stuff"]
                }
                assigned_node: 1
                revision: 4
                "#,
            )?,
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.p4rvyfqvb147y"
                    args: ["/bin/stuff"]
                }
                assigned_node: 2
                revision: 4
                "#,
            )?,
        ];
        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        let mut node3 = node1.clone();
        node3.set_id(3u64);

        let mut node4 = node1.clone();
        node4.set_id(4u64);

        meta_client
            .cluster_table::<NodeMetadata>()
            .put(&node3)
            .await?;
        meta_client
            .cluster_table::<NodeMetadata>()
            .put(&node4)
            .await?;

        manager.run_once().await?;

        let expected_workers = vec![
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.mkz8jc57m5qge"
                    args: [
                        "/bin/stuff"
                    ]
                }
                assigned_node: 4
                revision: 4
                "#,
            )?,
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.nxzzqfbp3eayj"
                    args: ["/bin/stuff"]
                }
                assigned_node: 1
                revision: 4
                "#,
            )?,
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.p4rvyfqvb147y"
                    args: ["/bin/stuff"]
                }
                assigned_node: 2
                revision: 4
                "#,
            )?,
        ];
        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        request.spec_mut().set_replicas(2u32);
        manager.start_job_impl(&request).await?;

        // One of the workers will now get marked as drained.
        let expected_workers = vec![
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.mkz8jc57m5qge"
                    args: [
                        "/bin/stuff"
                    ]
                }
                assigned_node: 4
                revision: 4
                drain: true
                "#,
            )?,
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.nxzzqfbp3eayj"
                    args: ["/bin/stuff"]
                }
                assigned_node: 1
                revision: 4
                "#,
            )?,
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.p4rvyfqvb147y"
                    args: ["/bin/stuff"]
                }
                assigned_node: 2
                revision: 4
                "#,
            )?,
        ];
        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        assert_eq!(
            meta_client
                .cluster_table::<WorkerMetadata>()
                .list_by_node(1u64)
                .await?,
            vec![WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.nxzzqfbp3eayj"
                    args: ["/bin/stuff"]
                }
                assigned_node: 1
                revision: 4
                "#,
            )?,]
        );

        assert_eq!(
            meta_client
                .cluster_table::<WorkerMetadata>()
                .list_by_node(2u64)
                .await?,
            vec![WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "daemon.p4rvyfqvb147y"
                    args: ["/bin/stuff"]
                }
                assigned_node: 2
                revision: 4
                "#,
            )?,]
        );

        assert_eq!(
            meta_client
                .cluster_table::<WorkerMetadata>()
                .list_by_node(0u64)
                .await?,
            vec![]
        );

        assert_eq!(
            meta_client
                .cluster_table::<WorkerMetadata>()
                .list_by_node(3u64)
                .await?,
            vec![]
        );

        assert_eq!(
            meta_client
                .cluster_table::<WorkerMetadata>()
                .list_by_node(10u64)
                .await?,
            vec![]
        );

        // Drained entry should not be cleaned up if we did not verify DONE at the
        // latest revision.

        // Wrong revision and state
        meta_client
            .cluster_table()
            .put(&WorkerStateMetadata::parse_text(
                r#"
            worker_name: "daemon.mkz8jc57m5qge"
            state: READY
            worker_revision: 1
        "#,
            )?)
            .await?;

        manager.run_once().await?;
        manager.run_once().await?;

        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        // Right state, wrong revision
        meta_client
            .cluster_table()
            .put(&WorkerStateMetadata::parse_text(
                r#"
            worker_name: "daemon.mkz8jc57m5qge"
            state: DONE
            worker_revision: 1
        "#,
            )?)
            .await?;

        manager.run_once().await?;
        manager.run_once().await?;

        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        // Wrong state, right revision
        meta_client
            .cluster_table()
            .put(&WorkerStateMetadata::parse_text(
                r#"
                worker_name: "daemon.mkz8jc57m5qge"
                state: READY
                worker_revision: 4
                "#,
            )?)
            .await?;

        manager.run_once().await?;
        manager.run_once().await?;

        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        // Can now be reclaimed
        meta_client
            .cluster_table()
            .put(&WorkerStateMetadata::parse_text(
                r#"
                worker_name: "daemon.mkz8jc57m5qge"
                state: DONE
                worker_revision: 4
                "#,
            )?)
            .await?;

        manager.run_once().await?;
        manager.run_once().await?;

        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            vec![
                WorkerMetadata::parse_text(
                    r#"
                    spec {
                        name: "daemon.nxzzqfbp3eayj"
                        args: ["/bin/stuff"]
                    }
                    assigned_node: 1
                    revision: 4
                    "#,
                )?,
                WorkerMetadata::parse_text(
                    r#"
                    spec {
                        name: "daemon.p4rvyfqvb147y"
                        args: ["/bin/stuff"]
                    }
                    assigned_node: 2
                    revision: 4
                    "#,
                )?,
            ]
        );

        assert_eq!(
            meta_client
                .cluster_table::<WorkerStateMetadata>()
                .list()
                .await?,
            vec![]
        );

        Ok(())
    }

    #[testcase]
    async fn uses_different_ports_on_a_node() -> Result<()> {
        let rng = Arc::new(crypto::random::ChaCha20RNG::new());
        let meta = TestMetastore::create().await?;

        let meta_client = meta.create_client().await?;

        // TODO: Add a valid last_seen
        let node1 = NodeMetadata::parse_text(
            r#"
            id: 1
            state: ACTIVE
            address: "10.100.0.101:10400"
            allocatable_port_range {
                start: 8000
                end: 9000
            }
        "#,
        )?;

        meta_client
            .cluster_table::<NodeMetadata>()
            .put(&node1)
            .await?;

        let mut request = StartJobRequest::parse_text(
            r#"
            spec {
                name: "server1"
                replicas: 2
                worker {
                    args: ["/bin/serve_a"]
                    ports {
                        name: "first_port"
                        type: TCP
                        protocol: HTTP
                    }
                    ports {
                        name: "second_port"
                        type: TCP
                        protocol: HTTP
                    }
                }
            }
            "#,
        )?;

        let manager = Manager::new(
            Arc::new(meta.create_client().await?),
            Arc::new(crypto::random::ChaCha20RNG::new()), // Fixed seed
        );
        manager.start_job_impl(&request).await?;

        let mut expected_workers = vec![
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "server1.nxzzqfbp3eayj"
                    args: [
                        "/bin/serve_a"
                    ]
                    ports: [
                        {
                            name: "first_port"
                            number: 8002
                            type: TCP
                            protocol: HTTP
                        },
                        {
                            name: "second_port"
                            number: 8003
                            type: TCP
                            protocol: HTTP
                        }
                    ]
                }
                assigned_node: 1
                revision: 3                
                "#,
            )?,
            WorkerMetadata::parse_text(
                r#"
                spec {
                    name: "server1.p4rvyfqvb147y"
                    args: [
                        "/bin/serve_a"
                    ]
                    ports: [
                        {
                            name: "first_port"
                            number: 8000
                            type: TCP
                            protocol: HTTP
                        },
                        {
                            name: "second_port"
                            number: 8001
                            type: TCP
                            protocol: HTTP
                        }
                    ]
                }
                assigned_node: 1
                revision: 3
                "#,
            )?,
        ];
        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers
        );

        // Create a second job.
        let mut request = StartJobRequest::parse_text(
            r#"
            spec {
                name: "server2"
                replicas: 1
                worker {
                    args: ["/bin/serve_b"]
                    ports {
                        name: "third_port"
                        type: TCP
                        protocol: HTTP
                    }
                }
            }
            "#,
        )?;
        manager.start_job_impl(&request).await?;

        // Uses another new port.
        let mut expected_workers2 = expected_workers.clone();
        expected_workers2.extend_from_slice(&[WorkerMetadata::parse_text(
            r#"
            spec {
                name: "server2.mkz8jc57m5qge"
                args: [
                    "/bin/serve_b"
                ]
                ports: [
                    {
                        name: "third_port"
                        number: 8004
                        type: TCP
                        protocol: HTTP
                    }
                ]
            }
            assigned_node: 1
            revision: 5
            "#,
        )?]);
        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers2
        );

        // Updating the job should re-use port numbers (associated with same port name).
        request.spec_mut().set_replicas(2u32);
        request.spec_mut().worker_mut().add_args("-v".into());

        request.spec_mut().worker_mut().ports_mut().insert(
            0,
            WorkerSpec_Port::parse_text(r#" name: "first_port" type: TCP protocol: HTTP "#)?,
        );

        manager.start_job_impl(&request).await?;

        let mut expected_workers2 = expected_workers.clone();
        expected_workers2.extend_from_slice(&[
            WorkerMetadata::parse_text(
                r#"
            spec {
                name: "server2.f6q4ytddj054c"
                args: [
                    "/bin/serve_b",
                    "-v"
                ]
                ports: [
                    {
                        name: "first_port"
                        number: 8006
                        type: TCP
                        protocol: HTTP
                    },
                    {
                        name: "third_port"
                        number: 8007
                        type: TCP
                        protocol: HTTP
                    }
                ]
            }
            assigned_node: 1
            revision: 7
            "#,
            )?,
            WorkerMetadata::parse_text(
                r#"
            spec {
                name: "server2.mkz8jc57m5qge"
                args: [
                    "/bin/serve_b",
                    "-v"
                ]
                ports: [
                    {
                        name: "first_port"
                        number: 8005
                        type: TCP
                        protocol: HTTP
                    },
                    {
                        name: "third_port"
                        number: 8004  # Same number as before
                        type: TCP
                        protocol: HTTP
                    }
                ]
            }
            assigned_node: 1
            revision: 7
            "#,
            )?,
        ]);

        assert_eq!(
            meta_client.cluster_table::<WorkerMetadata>().list().await?,
            expected_workers2
        );

        Ok(())
    }

    // Creating 2 jobs with ports will allocate different port numbers on the
    // same node. ^ also verify that updating these

    // TODO: Eventually snapshot stable states in production and verify that new
    // manager changes don't trigger diffs.

    // Test that when a node dies, we can reschedule elsewhere.

    /*
    Other things to test:
    - Test AllocateBlob
    - Test scheduling.distinct_nodes
    - Disallow providing 'spec.worker.name'
    */
}
