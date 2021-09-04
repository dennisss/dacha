use common::bytes::Bytes;
use common::errors::*;

#[derive(Clone, Debug)]
pub struct KeyValueEntry {
    pub key: Bytes,
    pub value: Bytes,
}

#[async_trait]
pub trait Iterable: Send + 'static {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>>;

    async fn seek(&mut self, key: &[u8]) -> Result<()>;
}
