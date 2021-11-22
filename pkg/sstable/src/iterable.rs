use common::bytes::Bytes;
use common::errors::*;

#[derive(Clone, Debug)]
pub struct KeyValueEntry {
    pub key: Bytes,
    pub value: Bytes,
}

#[async_trait]
pub trait Iterable<V>: Send + 'static {
    async fn next(&mut self) -> Result<Option<V>>;

    async fn seek(&mut self, key: &[u8]) -> Result<()>;
}
