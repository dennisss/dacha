use common::errors::*;
use protobuf_builtins::google::protobuf::Any;

#[async_trait]
pub trait Entity: 'static + Send + Sync {
    async fn config(&self) -> Result<Any>;

    async fn state(&self) -> Result<Any>;

    async fn update(&self, proposed_state: &Any) -> Result<()>;
}
