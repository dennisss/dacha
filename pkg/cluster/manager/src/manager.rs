use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use cluster_client::acl::principal::Principal;
use cluster_client::meta::*;
use cluster_client::service::address::ServiceName;
use common::errors::*;
use common::hash::FastHasherBuilder;
use crypto::random::{SharedRng, SharedRngExt};
use db_txn_client::run_transaction;
use db_table::db::{ProtobufDB, ProtobufDBTransaction};
use db_table::{query, query_one, raw_query, primary_key_prefix};
use protobuf::Message;
use rpc_util::{AddReflection, NamedPortArg};
use cluster_proto::cluster::*;
use builder_proto::builder::Platform;

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
    zone: String,
    db: Arc<ProtobufDB>,
    rng: Arc<dyn SharedRng>,
    log_timestamps: bool,
}

impl Manager {
    pub fn new(zone: &str, db: Arc<ProtobufDB>, rng: Arc<dyn SharedRng>) -> Self {
        Self {
            zone: zone.into(),
            db,
            rng,
            log_timestamps: true,
        }
    }

    /// Entrypoint of the background manager thread which periodically ensures
    /// that the cluster is in a good state.
    pub async fn run(self) -> Result<()> {
        // TODO: Require holding a metastore lock to running this loop (mainly to avoid
        // contention).
        loop {
            self.run_once().await?;

            // TODO: Reconcile a job immediately if some drained workers get marked as done.
            executor::sleep(JOB_RECONCILE_RETRY_INTERVAL).await;
        }

        Ok(())
    }

    async fn run_once(&self) -> Result<()> {
        let mut jobs = self.db.list::<JobMetadataTable>().await?;
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

        run_transaction!(self.db, txn, {
            self.start_job_transaction(request, &mut txn).await?;
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

    /*
    TODO:
    If there are any nodes that are dead:
    (haven't seen a heartbeat in a while),
    - Find all WorkerStateMetadata for the node
    - Mark all as 'unknown'
    - Mark NodeMetadata as unknown


    */

    /// In a single metastore transaction, this adds a job to the cluster.
    async fn start_job_transaction(
        &self,
        request: &StartJobRequest,
        txn: &mut ProtobufDBTransaction<'_>,
    ) -> Result<()> {
        let existing_job = query_one!(
            txn,
            JobMetadataTable,
            "spec.name = ?",
            request.spec().name()
        );

        if existing_job.is_none() {
            // A job can only be created if there are no job whose name is a prefix of the
            // new job name (at segment boundaries).
            //
            // In other words, for every '[job_name_i]' in the cluster already,
            // '[job_name_i].' must not be a prefix of '[new_job_name].'
            let name_segments = request.spec().name().split('.').collect::<Vec<&str>>();
            for i in 1..name_segments.len() - 1 {
                let prefix = name_segments[0..i].join(".");
                if let Some(_) = query_one!(txn, JobMetadataTable, "spec.name = ?", &prefix) {
                    return Err(rpc::Status::invalid_argument(format!(
                        "A job already exists with a prefix with a new job name: {}",
                        prefix
                    ))
                    .into());
                }
            }
        }

        let mut job_meta = existing_job.unwrap_or_else(|| JobMetadata::default());

        job_meta.set_stopping(false);

        if job_meta.spec().worker() != request.spec().worker() {
            job_meta.set_worker_revision(txn.read_index().await);
        }

        job_meta.set_spec(request.spec().clone());

        if self.log_timestamps {
            job_meta.set_last_updated(SystemTime::now());
        }

        txn.put::<JobMetadataTable>(&job_meta).await?;

        Ok(())
    }

    async fn stop_job_impl(&self, request: &StopJobRequest) -> Result<()> {
        let changed = run_transaction!(self.db, txn, {
            self.stop_job_transaction(request, &mut txn).await?
        });

        if changed {
            self.reconcile_job(request.name()).await?;
        }

        Ok(())
    }

    async fn stop_job_transaction(
        &self,
        request: &StopJobRequest,
        txn: &mut ProtobufDBTransaction<'_>,
    ) -> Result<bool> {
        let mut job_meta = query_one!(
            txn,
            JobMetadataTable,
            "spec.name = ?",
            request.name()
        ).ok_or_else(|| {
            Error::from(rpc::Status::not_found("No such job found"))
        })?;

        if job_meta.stopping() {
            return Ok(false);
        }

        job_meta.set_stopping(true);

        if self.log_timestamps {
            job_meta.set_last_updated(SystemTime::now());
        }

        txn.put::<JobMetadataTable>(&job_meta).await?;

        Ok(true)
    }

    async fn reconcile_job(&self, job_name: &str) -> Result<()> {
        let mut txn = self.db.new_transaction().await?;

        let job = query_one!(&txn, JobMetadataTable, "spec.name = ?", job_name)
            .ok_or_else(|| err_msg("Job doesn't exist"))?;

        // TODO: This read operation will cause a lot of contention as nodes may be
        // simultaneously updating their status.
        let mut nodes = txn.list::<NodeMetadataTable>().await?;
        if nodes.is_empty() {
            // TODO: This may be problematic during initial bootstrapping of the cluster.
            return Err(err_msg("No nodes present"));
        }

        // Mapping from node id to the index of the NodeMetadata in 'nodes'.
        let mut nodes_by_id = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            nodes_by_id.insert(node.id(), i);
        }

        // TODO: Parallelize with the previous query.
        // TODO: We only really need to lock the rows that we are changing.
        let nodes_scheduling = txn.list::<NodeSchedulingMetadataTable>().await?;

        let mut nodes_scheduling_by_id = HashMap::new();
        for data in nodes_scheduling {
            nodes_scheduling_by_id.insert(data.node_id(), data);
        }

        let mut nodes = nodes
            .into_iter()
            .map(|node| {
                let mut sched = nodes_scheduling_by_id
                    .remove(&node.id())
                    .unwrap_or_default();
                sched.set_node_id(node.id());

                (node, sched)
            })
            .collect::<Vec<_>>();

        let mut existing_workers = query!(
            &txn,
            WorkerMetadataTable,
            "STARTS_WITH(spec.name, ?)",
            format!("{}.", job_name)
        );

        // TODO: Do not re-schedule drained workers if using distinct_nodes on the same
        // node until it is done being cleaned up.
        let mut drained_workers = existing_workers
            .extract_if(.., |worker| worker.drain())
            .collect::<Vec<_>>();

        existing_workers.retain(|worker| !worker.drain());

        // Indexes of all nodes (in the 'nodes' vector) which we will consider for
        // running workers in this job.
        let mut remaining_nodes = vec![];
        for i in 0..nodes.len() {
            // TODO: Can probably use u32 for this. 
            remaining_nodes.push(i);
        }

        // TODO: Filter out any nodes which are not healthy.

        let job_spec_filter = JobSpecNodeFilter::new(job.spec());
        remaining_nodes.retain(|i| {
            let node = &nodes[*i];
            job_spec_filter.matches(&node.0, &node.1)
        });

        // TODO: Also limit the max number of workers per node.

        let mut remaining_uncordoned_nodes = remaining_nodes.clone();
        remaining_uncordoned_nodes.retain(|i| {
            !nodes[*i].1.cordoned()
        });

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

        let mut touched_node_ids = HashSet::new();

        // TODO: Any workers in a DONE state (or a RestartPolicy preventing from than
        // one )

        let mut target_replicas = job.spec().replicas() as usize;
        if job.stopping() {
            target_replicas = 0;
        }

        // TODO: Implement each replica as a separate transaction.
        for i in 0..target_replicas {
            // Attempt to select an existing worker that we want to re-use.
            let existing_worker = {
                let mut picked_worker = None;
                while let Some(worker) = existing_workers.pop() {
                    // The existing worker must still be in our selected node subset to be
                    // re-used.
                    if !remaining_nodes
                        .iter()
                        .find(|idx| nodes[**idx].0.id() == worker.assigned_node())
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
                    if remaining_uncordoned_nodes.is_empty() {
                        update_incomplete = true;
                        break;
                    }

                    let selected_idx = self.rng.between::<usize>(0, remaining_uncordoned_nodes.len()).await;
                    remaining_uncordoned_nodes[selected_idx]
                }
            };

            // If we are only allowed to assign to distinct nodes, remove the selected node
            // for the node set for future decisions.
            if job.spec().scheduling().distinct_nodes() {
                remaining_nodes.retain(|idx| *idx != assigned_node_index);
                remaining_uncordoned_nodes.retain(|idx| *idx != assigned_node_index);
            }

            // Skip if the existing worker is already up to date.
            if let Some(existing_worker) = &existing_worker {
                if existing_worker.revision() == job.worker_revision() {
                    continue;
                }
            }

            let assigned_node = &mut nodes[assigned_node_index];

            let mut new_worker = WorkerMetadata::default();
            new_worker.set_assigned_node(assigned_node.0.id());

            let new_spec = self
                .create_allocated_worker_spec(
                    job.spec().name(),
                    &job.spec().worker(),
                    existing_worker.as_ref().map(|t| t.spec()),
                    &assigned_node.0,
                    &assigned_node.1,
                )
                .await?;
            new_worker.set_spec(new_spec);
            new_worker.set_revision(job.worker_revision());
            if self.log_timestamps {
                new_worker.set_last_updated(SystemTime::now());
            }


            // Update the worker
            txn.put::<WorkerMetadataTable>(&new_worker).await?;

            touched_node_ids.insert(new_worker.assigned_node());

            // Make the node a replica of all referenced blobs.
            // TODO: Eventually check ahead of time that there is space to fit the blobs.
            for volume in new_worker.spec().volumes() {
                if !volume.has_bundle() {
                    continue;
                }

                let node_platform = assigned_node.0.platform();
                
                let mut blob_id = None;

                // TODO: Deduplicate this selection logic with the node.
                for variant in volume.bundle().variants() {
                    if variant.platform() == node_platform {
                        blob_id = Some(variant.blob().id().to_string());
                        break;
                    }
                }

                let blob_id = match blob_id {
                    Some(v) => v,
                    None => continue
                };   

                let replica_data = query_one!(txn, BundleBlobReplicaTable, "blob_id = ? AND node_id = ?",
                    &blob_id, assigned_node.0.id());
                if replica_data.is_none() {
                    self.create_blob_replica(&blob_id, assigned_node.0.id(), &mut txn).await?;
                }
            }

            // Authorize the assigned node to write to the WorkerStateMetadata row for this
            // worker.
            if existing_worker.is_none() {
                // TODO: Make a utility for doing this.

                let key = primary_key_prefix!(
                    WorkerStateMetadataTable,
                    "worker_name = ?",
                    new_worker.spec().name()
                );

                let mut proto = KeyPrefixACLProto::default();
                proto.set_prefix(key);
                proto.add_writers(
                    Principal::Entity(ServiceName::for_node(
                        &self.zone,
                        new_worker.assigned_node(),
                    )?)
                    .to_string(),
                );

                txn.put::<KeyPrefixACLTable>(&proto).await?;
            }

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
                        assigned_node.1.allocated_ports_mut().insert(port.number());
                        dirty = true;
                    }
                }

                for old_port in old_port_nums {
                    assigned_node.1.allocated_ports_mut().remove(&old_port);
                    dirty = true;
                }

                if dirty {
                    txn.put::<NodeSchedulingMetadataTable>(&assigned_node.1)
                        .await?;
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
            if self.log_timestamps {
                existing_worker.set_last_updated(SystemTime::now());
            }
            
            touched_node_ids.insert(existing_worker.assigned_node());

            txn.put::<WorkerMetadataTable>(&existing_worker).await?;

            let node = &mut nodes[*nodes_by_id
                .get(&existing_worker.assigned_node())
                .ok_or_else(|| err_msg("Failed to find assigned node"))?];

            // TODO: Don't de-allocate resources until the workers are fully cleaned up.
            let mut dirty = false;
            for port in existing_worker.spec().ports() {
                node.1.allocated_ports_mut().remove(&port.number());
                dirty = true;
            }

            if dirty {
                txn.put::<NodeSchedulingMetadataTable>(&node.1).await?;
            }
        }

        // TODO: Think about whether or not this is good enough (not having a read lock on the old NodeRevision rows).
        let read_index = txn.read_index().await;
        for node_id in touched_node_ids {
            let mut revision = NodeRevision::default();
            revision.set_node_id(node_id);
            revision.set_revision(read_index);
            txn.put::<NodeRevisionTable>(&revision).await?;
        }

        txn.commit().await?;

        self.cleanup_drained(&drained_workers).await?;

        if job.stopping() {
            self.cleanup_stopping_job(job_name).await?;
        }

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
        let mut txn = self.db.new_transaction().await?;

        for worker in drained_workers {
            let state_meta = query_one!(
                &txn,
                WorkerStateMetadataTable,
                "worker_name = ?",
                worker.spec().name()
            );

            let state_meta = match state_meta {
                Some(v) => v,
                None => continue,
            };

            if state_meta.state() == WorkerStateMetadata_ReportedState::DONE
                && state_meta.worker_revision() == worker.revision()
            {
                txn.remove::<WorkerMetadataTable>(worker).await?;
                txn.remove::<WorkerStateMetadataTable>(&state_meta).await?;
                // TODO: Also clean up the ACL records?
            }
        }

        txn.commit().await?;

        Ok(())
    }

    // TODO: Consider batching this with the other transaction in cleanup_drained.
    async fn cleanup_stopping_job(&self, job_name: &str) -> Result<()> {
        let mut txn = self.db.new_transaction().await?;

        let job = match query_one!(&txn, JobMetadataTable, "spec.name = ?", job_name) {
            Some(v) => v,
            None => return Ok(())
        };

        if !job.stopping() {
            return Ok(());
        }

        // TODO: Optimize to just fetch the count.
        let existing_workers = query!(
            &txn,
            WorkerMetadataTable,
            "STARTS_WITH(spec.name, ?)",
            format!("{}.", job_name)
        );

        // Can only delete the job once all workers have been fully drained.
        if existing_workers.len() > 0 {
            return Ok(());
        }

        txn.remove::<JobMetadataTable>(&job).await?;

        txn.commit().await?;
        Ok(())
    }

    // TODO: We' should avoid allocating the same ports very frequetly. we will also
    // need to validate that clients don't accidentally contact the wrong server by
    // checking the dns name requested (probably doable at the TLS level)

    /// Creates a worker
    async fn create_allocated_worker_spec(
        &self,
        job_name: &str,
        job_worker_spec: &WorkerSpec,
        old_spec: Option<&WorkerSpec>,
        node: &NodeMetadata,
        node_scheduling: &NodeSchedulingMetadata,
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
            name.push_str(&crate::new_worker_id(self.rng.as_ref()).await);
            name
        };

        spec.set_name(worker_name.as_str());

        // Expected new value fo the node's allocated port set after applying changes
        // in the created worker spec.
        let mut node_allocated_ports = HashSet::<u32, FastHasherBuilder>::default();
        for port in node_scheduling.allocated_ports().iter() {
            node_allocated_ports.insert(*port);
        }

        // Assign any ports that can be re-used from the past run.
        for port in spec.ports_mut() {
            port.clear_number();

            let old_spec = match old_spec.clone() {
                Some(v) => v,
                None => continue
            };

            let old_port = match old_spec.ports().iter().find(|v| v.name() == port.name()) {
                Some(v) => v,
                None => continue
            };

            let can_reuse = {
                if port.node_port() != 0 {
                    port.node_port() == old_port.number()
                } else {
                    old_port.number() >= node.allocatable_port_range().start() &&
                    old_port.number() < node.allocatable_port_range().end()
                }
            };

            if can_reuse {
                port.set_number(old_port.number());
            } else {
                node_allocated_ports.remove(&old_port.number());
            }
        }

        // Newly assign newly referenced ports.
        for port in spec.ports_mut() {
            if port.number() != 0 {
                continue;
            }

            if port.node_port() != 0 {
                if !node_allocated_ports.insert(port.node_port()) {
                    // TODO: Using a node_port should imply schedulign on distinct nodes.
                    return Err(format_err!("Node port {} already allocated on the node.", port.node_port()));
                }

                let p = port.node_port();
                port.set_number(p);
                continue;
            }

            // Otherwise, allocate a new port on the node.

            let mut found_port_num = false;
            for port_num in
                node.allocatable_port_range().start()..node.allocatable_port_range().end()
            {
                if node_allocated_ports.contains(&port_num) {
                    continue;
                }

                port.set_number(port_num);
                node_allocated_ports.insert(port_num);
                found_port_num = true;
                break;
            }

            if port.number() == 0 {
                // TODO: Raise this to the user in some schedulability report.
                return Err(err_msg("Failed to allocate a new port number"));
            }
        }

        Ok(spec)
    }

    async fn allocate_blobs_impl<'a>(
        &self,
        request: rpc::ServerRequest<AllocateBundleBlobsRequest>,
        response: &mut rpc::ServerResponse<'a, AllocateBundleBlobsResponse>,
    ) -> Result<()> {
        // TODO: Filter out unhealthy nodes.
        let mut nodes = self.db.list::<NodeMetadataTable>().await?;
        self.rng.shuffle(&mut nodes).await;

        // TODO: Parallelize with the previous query.
        let nodes_scheduling = self.db.list::<NodeSchedulingMetadataTable>().await?;

        let mut nodes_scheduling_by_id = HashMap::new();
        for data in nodes_scheduling {
            nodes_scheduling_by_id.insert(data.node_id(), data);
        }

        let mut job_node_filter = None;
        if request.has_job_spec() {
            job_node_filter = Some(JobSpecNodeFilter::new(request.job_spec()));
        }

        let mut txn = self.db.new_transaction().await?;

        for spec in request.blob_specs() {
            // TODO: Validate the blob id format.

            let mut blob = query_one!(txn, BundleBlobMetadataTable, "spec.id = ?", spec.id())
                .unwrap_or_else(|| {
                    let mut b = BundleBlobMetadata::default();
                    b.set_spec(spec.as_ref().clone());
                    b
                });

            let mut blob_replicas = query!(txn, BundleBlobReplicaTable, "blob_id = ?", spec.id());

            blob_replicas.retain(|blob_replica| {
                // Ignore cordoned nodes.
                if let Some(node_scheduling) = nodes_scheduling_by_id.get(&blob_replica.node_id()) {
                    if !node_scheduling.cordoned() {
                        return false;
                    }
                }

                // TODO: Implement this.
                /*
                if let Some(job_filter) = &job_node_filter {
                    if !job_filter.matches() {
                        return false;
                    }
                }
                */

                true
            });

            let mut num_uploaded = 0;

            let mut existing_node_ids = HashSet::new();
            for replica in &blob_replicas {
                existing_node_ids.insert(replica.node_id());

                if replica.uploaded() {
                    num_uploaded += 1;
                }
            }

            if num_uploaded > 0 {
                continue;
            }

            // Ensure there are enough replicas defined for the blob. 
            const MIN_REPLICAS: usize = 1;
            while blob_replicas.len() < MIN_REPLICAS {
                let mut new_node_id = None;
                for node in &nodes {
                    if existing_node_ids.contains(&node.id()) {
                        continue;
                    }

                    if let Some(job_node_filter) = &job_node_filter {
                        let node_scheduling = match nodes_scheduling_by_id.get(&node.id()) {
                            Some(v) => v,
                            None => continue
                        };

                        if !job_node_filter.matches(node, node_scheduling) {
                            continue;
                        }
                    }

                    new_node_id = Some(node.id());
                    break;
                }

                let new_node_id = new_node_id.ok_or_else(|| err_msg("Failed to get a node"))?;
                blob_replicas.push(self.create_blob_replica(spec.id(), new_node_id, &mut txn).await?);
            }

            // Ask the client to push the blob to some subset of the replicas that don't have the
            // data.
            let mut num_pushing = 0;
            for replica in &blob_replicas {
                if replica.uploaded() {
                    continue;
                }

                if num_pushing == 2 {
                    break;
                }
                num_pushing += 1;

                let mut assignment = BundleBlobAssignment::default();
                assignment.set_blob_id(spec.id());
                assignment.set_node_id(replica.node_id());
                response.value.add_new_assignments(assignment);
            }

            // This will insert the entry if one didn't already exist.
            txn.put::<BundleBlobMetadataTable>(&blob).await?;
        }

        // TODO: Update NodeRevisionTable

        txn.commit().await?;

        Ok(())
    }

    /// Assuming the entry doesn't already exist, creates a new BundleBlobReplica entry with the blob marked as not uploaded.
    async fn create_blob_replica(
        &self, blob_id: &str, node_id: u64, txn: &mut ProtobufDBTransaction<'_>
    ) -> Result<BundleBlobReplica> {
        let mut replica = BundleBlobReplica::default();
        replica.set_blob_id(blob_id);
        replica.set_node_id(node_id);
        replica.set_timestamp(std::time::SystemTime::now());

        // Allow the node to update its row in the BunbleBlobReplica table.
        {
            // TODO: Make a utility for doing this.

            let key = primary_key_prefix!(
                BundleBlobReplicaTable,
                "blob_id = ? AND node_id = ?",
                replica.blob_id(), replica.node_id()
            );

            let mut proto = KeyPrefixACLProto::default();
            proto.set_prefix(key);
            proto.add_writers(
                Principal::Entity(ServiceName::for_node(
                    &self.zone,
                    replica.node_id(),
                )?)
                .to_string(),
            );

            txn.put::<KeyPrefixACLTable>(&proto).await?;
        }

        txn.put::<BundleBlobReplicaTable>(&replica).await?;

        Ok(replica)
    }
}

struct JobSpecNodeFilter<'a> {
    job_spec: &'a JobSpec,
    supported_platforms: Option<HashSet<Vec<u8>>>,
}

impl<'a> JobSpecNodeFilter<'a> {
    fn new(job_spec: &'a JobSpec) -> Self {
        let supported_platforms = Self::get_supported_platforms(job_spec.worker());
        Self {
            job_spec,
            supported_platforms
        }
    }

    fn matches(
        &self,
        node_meta: &NodeMetadata,
        node_scheduling: &NodeSchedulingMetadata,
    ) -> bool {

        if self.job_spec.scheduling().specific_nodes_len() > 0 {
            let current_id = node_meta.id();

            if self.job_spec
                .scheduling()
                .specific_nodes()
                .iter()
                .find(|id| **id == current_id)
                .is_none() {
                return false;
            }

            // TODO: Validate that all the ids are valid somewhere
            /*
            if remaining_nodes.len() != job.spec().scheduling().specific_nodes_len() {
                return Err(err_msg("Some nodes in specific_nodes weren't found"));
            }
            */
        }

        if !self.filter_by_node_labels(node_meta, node_scheduling) {
            return false;
        }

        if let Some(supported_platforms) = &self.supported_platforms {
            if !supported_platforms.contains(&Self::platform_string(node_meta.platform())) {
                return false;
            }
        }

        true
    }

    /// Returns whether or not to keep a node based on a selector.
    fn filter_by_node_labels(
        &self,
        node_meta: &NodeMetadata,
        node_scheduling: &NodeSchedulingMetadata,
    ) -> bool {
        let selector = self.job_spec.scheduling().labels();

        let mut node_labels: HashMap<&str, &str> = HashMap::default();
        for label in node_scheduling.labels().label() {
            node_labels.insert(label.key(), label.value());
        }

        for label_selector in selector.label() {
            let value = match node_labels.get(&label_selector.key()) {
                Some(v) => *v,
                None => return false,
            };

            if label_selector.present() {
                continue;
            } else if !label_selector.value().is_empty() {
                if label_selector
                    .value()
                    .iter()
                    .find(|v| v.as_str() == value)
                    .is_none()
                {
                    return false;
                }
            } else {
                // Unknown or new selector.
                continue;
            }
        }

        true
    }


    fn platform_string(platform: &Platform) -> Vec<u8> {
        let mut out = vec![];
        platform.serialize_to(&protobuf::SerializeOptions::default(), &mut out).unwrap();
        out
    }

    // Will return None if all platforms are supported.
    fn get_supported_platforms(job_worker_spec: &WorkerSpec) -> Option<HashSet<Vec<u8>>> {
        let mut num_bundles = 0;
        let mut seen_platforms: HashMap<Vec<u8>, usize> = HashMap::new();

        for volume in job_worker_spec.volumes() {
            if !volume.has_bundle() {
                continue;
            }

            num_bundles += 1;

            for variant in volume.bundle().variants() {
                *seen_platforms.entry(Self::platform_string(&variant.platform())).or_default() += 1;
            }
        }

        if num_bundles == 0 {
            return None;
        }

        let mut out = HashSet::new();
        for (key, value) in seen_platforms {
            if value == num_bundles {
                out.insert(key);
            }
        }

        Some(out)
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

    async fn StopJob(
        &self,
        request: rpc::ServerRequest<StopJobRequest>,
        response: &mut rpc::ServerResponse<StopJobResponse>,
    ) -> Result<()> {
        self.stop_job_impl(&request.value).await
    }

    async fn AllocateBundleBlobs(
        &self,
        request: rpc::ServerRequest<AllocateBundleBlobsRequest>,
        response: &mut rpc::ServerResponse<AllocateBundleBlobsResponse>,
    ) -> Result<()> {
        self.allocate_blobs_impl(request, response).await
    }
}

#[cfg(test)]
mod tests {
    use db_txn::TestMetastore;
    use protobuf::text::ParseTextProto;

    use super::*;

    #[testcase]
    async fn can_add_a_job() -> Result<()> {
        let rng = Arc::new(crypto::random::ChaCha20RNG::new());
        let meta = TestMetastore::create().await?;

        let meta_client = meta.create_client().await?;
        let db = Arc::new(ProtobufDB::new(Arc::new(meta_client)));

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

        db.insert::<NodeMetadataTable>(&node1).await?;

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

        let mut manager = Manager::new(
            "testing",
            db.clone(),
            Arc::new(crypto::random::ChaCha20RNG::new()), // Fixed seed
        );
        manager.log_timestamps = false;
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

        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        // Verify that doing more iterations doesn't change anything.
        manager.run_once().await?;
        manager.run_once().await?;
        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        // Start it again (should be idempotent)
        manager.start_job_impl(&request).await?;

        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

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
        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        Ok(())
    }

    #[testcase]
    async fn job_on_distinct_nodes() -> Result<()> {
        let rng = Arc::new(crypto::random::ChaCha20RNG::new());
        let meta = TestMetastore::create().await?;

        let meta_client = meta.create_client().await?;
        let db = Arc::new(ProtobufDB::new(Arc::new(meta_client)));

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

        db.insert::<NodeMetadataTable>(&node1).await?;
        db.insert::<NodeMetadataTable>(&node2).await?;

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

        let mut manager = Manager::new(
            "testing",
            db.clone(), // TODO: Always use an independent client interface?
            Arc::new(crypto::random::ChaCha20RNG::new()), // Fixed seed
        );
        manager.log_timestamps = false;
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
        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        let mut node3 = node1.clone();
        node3.set_id(3u64);

        let mut node4 = node1.clone();
        node4.set_id(4u64);

        db.insert::<NodeMetadataTable>(&node3).await?;
        db.insert::<NodeMetadataTable>(&node4).await?;

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
        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

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
        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        assert_eq!(
            query!(db, WorkerMetadataTable, "assigned_node = ?", 1u64),
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
            query!(db, WorkerMetadataTable, "assigned_node = ?", 2u64),
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
            query!(db, WorkerMetadataTable, "assigned_node = ?", 0u64),
            vec![]
        );

        assert_eq!(
            query!(db, WorkerMetadataTable, "assigned_node = ?", 3u64),
            vec![]
        );

        assert_eq!(
            query!(db, WorkerMetadataTable, "assigned_node = ?", 10u64),
            vec![]
        );

        // Drained entry should not be cleaned up if we did not verify DONE at the
        // latest revision.

        // Wrong revision and state
        db.insert::<WorkerStateMetadataTable>(&WorkerStateMetadata::parse_text(
            r#"
            worker_name: "daemon.mkz8jc57m5qge"
            state: READY
            worker_revision: 1
        "#,
        )?)
        .await?;

        manager.run_once().await?;
        manager.run_once().await?;

        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        // Right state, wrong revision
        db.insert::<WorkerStateMetadataTable>(&WorkerStateMetadata::parse_text(
            r#"
            worker_name: "daemon.mkz8jc57m5qge"
            state: DONE
            worker_revision: 1
        "#,
        )?)
        .await?;

        manager.run_once().await?;
        manager.run_once().await?;

        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        // Wrong state, right revision
        db.insert::<WorkerStateMetadataTable>(&WorkerStateMetadata::parse_text(
            r#"
                worker_name: "daemon.mkz8jc57m5qge"
                state: READY
                worker_revision: 4
                "#,
        )?)
        .await?;

        manager.run_once().await?;
        manager.run_once().await?;

        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

        // Can now be reclaimed
        db.insert::<WorkerStateMetadataTable>(&WorkerStateMetadata::parse_text(
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
            db.list::<WorkerMetadataTable>().await?,
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

        assert_eq!(db.list::<WorkerStateMetadataTable>().await?, vec![]);

        Ok(())
    }

    #[testcase]
    async fn uses_different_ports_on_a_node() -> Result<()> {
        let rng = Arc::new(crypto::random::ChaCha20RNG::new());
        let meta = TestMetastore::create().await?;

        let meta_client = meta.create_client().await?;
        let db = Arc::new(ProtobufDB::new(Arc::new(meta_client)));

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

        db.insert::<NodeMetadataTable>(&node1).await?;

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
                    }
                    ports {
                        name: "second_port"
                        type: TCP
                    }
                }
            }
            "#,
        )?;

        let mut manager = Manager::new(
            "testing",
            db.clone(),
            Arc::new(crypto::random::ChaCha20RNG::new()), // Fixed seed
        );
        manager.log_timestamps = false;
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
                        },
                        {
                            name: "second_port"
                            number: 8003
                            type: TCP
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
                        },
                        {
                            name: "second_port"
                            number: 8001
                            type: TCP
                        }
                    ]
                }
                assigned_node: 1
                revision: 3
                "#,
            )?,
        ];
        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers);

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
                    }
                ]
            }
            assigned_node: 1
            revision: 5
            "#,
        )?]);
        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers2);

        // Updating the job should re-use port numbers (associated with same port name).
        request.spec_mut().set_replicas(2u32);
        request.spec_mut().worker_mut().add_args("-v".into());

        request.spec_mut().worker_mut().ports_mut().insert(
            0,
            WorkerSpec_Port::parse_text(r#" name: "first_port" type: TCP "#)?.into(),
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
                    },
                    {
                        name: "third_port"
                        number: 8007
                        type: TCP
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
                    },
                    {
                        name: "third_port"
                        number: 8004  # Same number as before
                        type: TCP
                    }
                ]
            }
            assigned_node: 1
            revision: 7
            "#,
            )?,
        ]);

        assert_eq!(db.list::<WorkerMetadataTable>().await?, expected_workers2);

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
    - node_port
    - draining workers
    - stopping a job
    */
}
