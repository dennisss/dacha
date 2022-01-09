use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use common::errors::*;
use common::errors::*;
use crypto::random::RngExt;
use datastore::meta::client::MetastoreClient;
use datastore::meta::client::MetastoreClientInterface;
use datastore::meta::client::MetastoreTransaction;
use protobuf::Message;
use rpc_util::{AddReflection, NamedPortArg};

use crate::meta::client::ClusterMetaClient;
use crate::meta::GetClusterMetaTable;
use crate::proto::blob::*;
use crate::proto::job::*;
use crate::proto::manager::*;
use crate::proto::meta::*;
use crate::proto::task::*;

/*
When a manager test starts up, it will
- Acquire a metastore lock under `/system/manager/lock`
  - If it can't it will sleep for 30 seconds and try again.
- Enumerate all Job instances in the database.
  - For each job instance, verify that they there are tasks for each job assigned to nodes.
- Finally, loop through each Node and ensure that it has all required nodes.
  ^ After the initial

- Want to have an active connection to each node to receive change updates.


Manager Role:
- Keep the metadata store alive
- Ping the nodes and see that they have the
- Ensure that every job has all its tasks to some node
    - If a node is dead, we may want to move all of its tasks to another node (assuming they
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
    - Verifies they are running the right tasks.
    - When tasks become ready, the manager will mark them as ready/not-ready in the metadata store.
        -> Issue is that this is fragile?
    - TODO: Replace with just having the node watch for updates?

Should notds support pulling blobs from our servers?
- Yes because that is more efficient.

*/

regexp!(JOB_NAME_PATTERN => "^((?:[a-z](?:[a-z0-9\\-_]*[a-z0-9])?)\\.?)+$");

/// The max length of a URL is 255 characters.
/// It's somewhat difficult to verify that the name won't cause an overflow in
/// all contexts, so just to be safe, we won't allow jobs with names close to
/// that limit (minus a buffer for DNS names, task ids, etc.)
const JOB_NAME_MAX_SIZE: usize = 180;

const JOB_NAME_MAX_LABEL_LENGTH: usize = 63;

pub struct Manager {
    meta_client: Arc<ClusterMetaClient>,
}

impl Manager {
    pub fn new(meta_client: Arc<ClusterMetaClient>) -> Self {
        Self { meta_client }
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

            if !spec.task().name().is_empty() {
                return Err(
                    rpc::Status::invalid_argument("Not allowed to specify a task name").into(),
                );
            }

            if !Self::is_valid_job_name(spec.name()) {
                return Err(rpc::Status::invalid_argument("Invalid job name").into());
            }

            for port in spec.task().ports() {
                if port.number() != 0 {
                    return Err(rpc::Status::invalid_argument(
                        "Not allowed to specify port numbers",
                    )
                    .into());
                }
            }

            // TODO: Require authentication to create system services.
            if spec.task().persistent() && !spec.name().starts_with("system.") {
                return Err(rpc::Status::invalid_argument(
                    "Not allowed to specify persistent task flag.",
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
        self.reconcile_job(request.spec().name()).await?;

        // Trigger re-calculation of the tasks.
        // - Look up the job
        // - Look up all tasks associated with the job (ideally transactionally).
        // - If we need more tasks, look up all nodes and try to find one .
        // -

        // Thread 1: React to changes in individual jobs. Re-calculate requirements.
        // - If we need to

        // /cluster/task/[task_name]
        // /cluster/task_by_node/[node_id]

        // For each node, we do want to track:
        // - Assigned resources
        // - Assigned task names.

        Ok(())
    }

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

        if job_meta.spec().task() != request.spec().task() {
            job_meta.set_task_revision(txn.read_index().await);
        }

        job_meta.set_spec(request.spec().clone());

        job_table.put(&job_meta).await?;

        Ok(())
    }

    async fn reconcile_job(&self, job_name: &str) -> Result<()> {
        let txn = self.meta_client.new_transaction().await?;

        let nodes_table = txn.cluster_table::<NodeMetadata>();
        let jobs_table = txn.cluster_table::<JobMetadata>();
        let tasks_table = txn.cluster_table::<TaskMetadata>();

        let job = jobs_table
            .get(job_name)
            .await?
            .ok_or_else(|| err_msg("Job doesn't exist"))?;

        let mut nodes = nodes_table.list().await?;
        if nodes.is_empty() {
            // TODO: This may be problematic during initial bootstrapping of the cluster.
            return Err(err_msg("No nodes present"));
        }

        // TODO: This read operation will cause a lot of contention.
        let mut nodes_by_id = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            nodes_by_id.insert(node.id(), i);
        }

        let mut existing_tasks = {
            let task_prefix = format!("{}.", job_name);
            tasks_table.get_prefix(&task_prefix).await?
        };

        // TODO: Need to increment ref counts to blobs.

        // TODO: Implement each replica as a separate transaction.
        for _ in 0..(job.spec().replicas() as usize) {
            let existing_task = existing_tasks.pop();

            let assigned_node_index = {
                if let Some(existing_task) = &existing_task {
                    // TODO: Verify that the existing node is still healthy (and we don't need to
                    // move the task to another node).

                    if existing_task.revision() == job.task_revision() {
                        continue;
                    }

                    *nodes_by_id
                        .get(&existing_task.assigned_node())
                        .ok_or_else(|| err_msg("Failed to find assigned node"))?
                } else {
                    crypto::random::clocked_rng().between::<usize>(0, nodes.len())
                }
            };

            let assigned_node = &mut nodes[assigned_node_index];

            let mut new_task = TaskMetadata::default();
            new_task.set_assigned_node(assigned_node.id());

            let new_spec = self.create_allocated_task_spec(
                job.spec().name(),
                &job.spec().task(),
                existing_task.as_ref().map(|t| t.spec()),
                assigned_node,
            )?;
            new_task.set_spec(new_spec);
            new_task.set_revision(job.task_revision());

            // Update the task
            tasks_table.put(&new_task).await?;

            // Update the node
            {
                let mut dirty = false;

                let mut old_port_nums = HashSet::new();
                if let Some(existing_task) = &existing_task {
                    for port in existing_task.spec().ports() {
                        old_port_nums.insert(port.number());
                    }
                }

                for port in new_task.spec().ports() {
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

        // TODO: We can't delete a task or switch it to another node until we know that
        // the node to which it was originally assigned has stopped the tasks (otherwise
        // we might end up re-assigning resources before they are available?)
        // - There is a similar issue when switch to a new task spec with conflicting
        //   requirements
        // - This should be solved if the Node is smart enough to handle resources and
        //   can mark tasks are Pending before they are schedulable
        // - For ports, we do need to ensure that we check host names to ensure that
        //   users are querying the right task.

        // Stop all extra instances.
        // TODO: Consider just marking as stopped instead of deleting them?
        for mut existing_task in existing_tasks {
            tasks_table.delete(&existing_task).await?;

            let node = &mut nodes[*nodes_by_id
                .get(&existing_task.assigned_node())
                .ok_or_else(|| err_msg("Failed to find assigned node"))?];

            let mut dirty = false;
            for port in existing_task.spec().ports() {
                node.allocated_ports_mut().remove(&port.number());
                dirty = true;
            }

            if dirty {
                nodes_table.put(node).await?;
            }
        }

        txn.commit().await?;

        // run_transaction!(&self.meta_client, txn, {

        // });

        /*
        TODO: We must support implementing constraints/preferences for tasks

        For now, we want to support the following constraints:
        - Unique nodes for each task
        -
        - Have different Node constraint
        - Have a

        Need to maintain how many
        */

        // Begin Transaction

        // Lookup the job

        // Lookup tasks associated with the job.

        // Check all N new ones.

        // If we need more, randomly pick more nodes

        // If we need less, delete some

        // Commit Transaction

        // Notify all effected nodes that their task set has changed.

        Ok(())
    }

    /// Creates a task
    ///
    /// TODO: This must mutate the allocated ports set so that we don't end up
    /// obtaining the same port for multiple separate ports.
    fn create_allocated_task_spec(
        &self,
        job_name: &str,
        job_task_spec: &TaskSpec,
        old_spec: Option<&TaskSpec>,
        node: &NodeMetadata,
    ) -> Result<TaskSpec> {
        let mut spec = job_task_spec.clone();

        let task_name = if let Some(spec) = &old_spec {
            spec.name().to_string()
        } else {
            format!("{}.{}", job_name, crate::manager::new_task_id())
        };

        spec.set_name(task_name.as_str());

        for port in spec.ports_mut() {
            // If updating an existing task, attempt to re-use existing port assignments.
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
                if node.allocated_ports().contains(&port_num) {
                    continue;
                }

                port.set_number(port_num);
                found_port_num = true;
                break;
            }

            if port.number() == 0 {
                return Err(err_msg("Failed to allocate a new port number"));
            }
        }

        for volume in spec.volumes_mut() {
            match volume.source_case() {
                TaskSpec_VolumeSourceCase::Unknown => {}
                TaskSpec_VolumeSourceCase::Bundle(_) => {}
                TaskSpec_VolumeSourceCase::PersistentName(name) => {
                    // Persistent volumes should be specific to individual tasks.
                    // TODO: Consider moving this local to the node?
                    // (or have a system for making persistent volume claims?)
                    volume.set_persistent_name(format!("{}/{}", task_name, name));
                }
                TaskSpec_VolumeSourceCase::BuildTarget(_) => {}
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
        crypto::random::clocked_rng().shuffle(&mut nodes);

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
