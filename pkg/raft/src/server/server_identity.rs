use common::errors::*;

use crate::proto::ServerId;

const FROM_KEY: &str = "raft-from";
const TO_KEY: &str = "raft-to";

#[derive(PartialEq, Eq, Clone)]
pub struct ServerIdentity {
    pub server_id: ServerId,
}

impl std::hash::Hash for ServerIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u64(self.server_id.value());
    }
}

impl ServerIdentity {
    pub fn new(server_id: ServerId) -> Self {
        Self { server_id }
    }

    pub(super) fn new_outgoing_request_context(
        &self,
        to_id: ServerId,
    ) -> Result<rpc::ClientRequestContext> {
        let mut context = rpc::ClientRequestContext::default();
        context
            .metadata
            .add_text(FROM_KEY, &self.server_id.value().to_string())?;
        context
            .metadata
            .add_text(TO_KEY, &to_id.value().to_string())?;
        Ok(context)
    }

    pub(super) fn new_anonymous_request_context(
        to_id: ServerId,
    ) -> Result<rpc::ClientRequestContext> {
        let mut context = rpc::ClientRequestContext::default();
        context
            .metadata
            .add_text(TO_KEY, &to_id.value().to_string())?;
        Ok(context)
    }

    pub(super) fn check_incoming_request_context(
        &self,
        request_context: &rpc::ServerRequestContext,
        response_context: &mut rpc::ServerResponseContext,
    ) -> Result<()> {
        /*
        TODO: Eventually also verify the TLS confirmed client credential
        */

        response_context
            .metadata
            .head_metadata
            .add_text(FROM_KEY, &self.server_id.value().to_string())?;

        // Record who sent us this message
        // TODO: Should receiving a message from one's self be an error?
        if let Some(h) = request_context.metadata.get_text(FROM_KEY)? {
            let from_id: ServerId = h
                .parse::<u64>()
                .map_err(|_| rpc::Status::invalid_argument("Invalid From id"))?
                .into();

            if from_id == self.server_id {
                return Err(rpc::Status::invalid_argument("Sending request to self").into());
            }

            // TODO: Consider exporting the from id this for internal metrics.
        }

        // Verify that we are the intended recipient of this message
        let verified_recipient = if let Some(h) = request_context.metadata.get_text(TO_KEY)? {
            let to_id: ServerId = h
                .parse::<u64>()
                .map_err(|_| rpc::Status::invalid_argument("Invalid To id"))?
                .into();

            if to_id != self.server_id {
                // Bail out. The client should adjust its routing info based on the
                // identity we return back in response metadata.
                return Err(rpc::Status::invalid_argument("Not the intended recipient").into());
            }

            true
        } else {
            false
        };

        if !verified_recipient {
            return Err(rpc::Status::invalid_argument(
                "Receipient must be specified to make this request",
            )
            .into());
        }

        Ok(())
    }
}
