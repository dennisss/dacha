use common::errors::*;

/*
When a manager test starts up, it will
- Acquire a metastore lock under `/system/manager/lock`
  - If it can't it will sleep for 30 seconds and try again.
- Enumerate all Job instances in the database.
  - For each job instance, verify that they there are tasks for each job assigned to nodes.
- Finally, loop through each Node and ensure that it has all required nodes.
  ^ After the initial


Manager Role:
- Keep the metadata store alive
- Ping the nodes and see that they have the
- Ensure that every job has all its tasks to some node
- Ensure that every blob has at least N replicas.
- Delete blobs that are not in use for at least N days.

*/

use common::errors::*;
use datastore::meta::client::MetastoreClient;
use protobuf::Message;

use crate::proto::meta::*;

// TODO: Should we support having multiple zones use a single metastore client?

pub struct Manager {
    meta_client: MetastoreClient,
}

struct ClusterSnapshot {}

impl Manager {
    async fn fetch_snapshot(&self) -> Result<ClusterSnapshot> {
        let jobs = self
            .meta_client
            .list_protos::<JobMetadata>("/cluster/job")
            .await?;
        let tasks = self
            .meta_client
            .list_protos::<TaskMetadata>("/cluster/task")
            .await?;
        let nodes = self
            .meta_client
            .list_protos::<NodeMetadata>("/cluster/node")
            .await?;
        let blobs = self
            .meta_client
            .list_protos::<BlobMetadata>("/cluster/blob")
            .await?;

        // Watch(prefix)

        Err(err_msg(""))
    }
}

/*
When a user wants to star a job:
- upload blob (basically add to any node one replica (more than one not needed until we have a task to put it on))
- Then tell the manager server that it has been uploaded so that there is knowledge of this (or just have it proxy the request?)
*/

pub async fn run() -> Result<()> {
    // Step 1: Create a metastore client.
    // Step 2: Acquire a manager lock
    // Step 3: Enumerate

    todo!()

    /*
    Threads:
    - RPC server
    - Change actuator.
        - Listened to events:
            - NewJob
            - NewBlob

    The blob protocol:
    - When a user wants to

    Should notes support pulling blobs from our servers?
    - Yes because that is more efficient.

    */
}
