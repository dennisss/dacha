use std::sync::Arc;

use common::errors::*;
use crypto::random::RngExt;

use crate::proto::consensus::*;
use crate::proto::server_metadata::GroupId;
use crate::server::channel_factory::*;
use crate::server::server_identity::ServerIdentity;

/// Creates a new unique server id.
/// This is done by reaching up to existing servers in the cluster and reaching
/// consensus on a unique unused id.
pub async fn generate_new_server_id(
    group_id: GroupId,
    channel_factory: &dyn ChannelFactory,
) -> Result<ServerId> {
    let mut request = ProposeRequest::default();
    request.set_wait(true);
    request.data_mut().set_noop(true);

    let proposal = propose_entry(group_id, channel_factory, &request).await?;

    // Casting LogIndex to ServerId.
    Ok(proposal.index().value().into())
}

pub(super) async fn propose_entry(
    group_id: GroupId,
    channel_factory: &dyn ChannelFactory,
    request: &ProposeRequest,
) -> Result<LogPosition> {
    let mut suspected_leader_id = None;
    loop {
        let leader_id = match suspected_leader_id.take() {
            Some(id) => id,
            None => {
                // TODO: Must support the known_ids list being empty.

                // If we have no idea who the leader is, pick a random server to ask.
                let known_ids = channel_factory
                    .reachable_servers()
                    .await?
                    .into_iter()
                    .collect::<Vec<_>>();
                let mut rng = crypto::random::clocked_rng();
                let id = rng.choose(&known_ids);
                *id
            }
        };

        let stub = ConsensusStub::new(channel_factory.create(leader_id).await?);

        let response = stub
            .Propose(
                &ServerIdentity::new_anonymous_request_context(group_id, leader_id)?,
                request,
            )
            .await;

        let value = match response.result {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to query server {}", e);
                common::async_std::task::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        if value.has_error() {
            println!("Proposal failed: {:?}", value.error());
            if value.error().not_leader().leader_hint().value() > 0 {
                suspected_leader_id = Some(value.error().not_leader().leader_hint());
            }

            common::async_std::task::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }

        return Ok(value.proposal().clone());
    }
}
