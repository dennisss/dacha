use std::sync::Arc;

use common::errors::*;
use crypto::random::RngExt;

use crate::log::log::Log;
use crate::log::log_metadata::LogSequence;
use crate::proto::consensus::*;
use crate::proto::ident::ServerId;
use crate::proto::server_metadata::GroupId;
use crate::server::channel_factory::*;
use crate::server::server_identity::ServerIdentity;

pub async fn bootstrap_first_server(log: &dyn Log) -> Result<ServerId> {
    let server_id = 1.into();

    // For this to be supported, we must be able to become a leader with zero
    // members in the config (implying that we can know if we are )
    let mut first_entry = LogEntry::default();
    first_entry.pos_mut().set_term(1);
    first_entry.pos_mut().set_index(1);
    first_entry.data_mut().config_mut().set_AddMember(server_id);

    let mut new_entries = vec![];
    new_entries.push(first_entry);

    let mut seq = LogSequence::zero();
    for e in new_entries {
        let next_seq = seq.next();
        seq = next_seq;

        log.append(e, next_seq).await?;
    }

    log.flush().await?;

    Ok(server_id)
}

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
                if known_ids.is_empty() {
                    println!("No servers discovered yet");
                    common::async_std::task::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }

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

/*
async fn init_server() {
    let mut node_uri = http::uri::Uri::from_str(&format!("http://{}", cmd.node_addr))?;
    node_uri.authority.as_mut().unwrap().port = Some(METASTORE_INITIAL_PORT as u16);

    let bootstrap_client = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
        &node_uri,
    )?)?);

    let stub = raft::ServerInitStub::new(bootstrap_client);

    // TODO: Ignore method not found errors (which would imply that we are already
    // bootstrapped).
    if let Err(e) = stub
        .Bootstrap(&request_context, &raft::BootstrapRequest::default())
        .await
        .result
    {
        if let Some(status) = e.downcast_ref::<rpc::Status>() {
            if status.code() == rpc::StatusCode::Unimplemented {
                // Likely the method doesn't exist so the metastore is probably already
                // bootstrapped.
                println!("=> Already bootstrapped");
            } else {
                return Err(e);
            }
        } else {
            return Err(e);
        }
    } else {
        println!("=> Done!");
    }

}
*/
