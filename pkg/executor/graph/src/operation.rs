use base_error::*;
use executor::bundle::TaskResultBundle;
use executor::channel::spsc::{self, Receiver, Sender};

use crate::stream::*;

#[derive(Clone, Debug)]
pub struct OperationSignature {
    pub name: String,
    pub num_inputs: usize,
    pub num_outputs: usize,
}

#[async_trait]
pub trait Operation: 'static + Send + Sync {
    fn signature(&self) -> OperationSignature;

    /// Executes the operation on the given streams of inputs and writes the
    /// results to the given output streams.
    ///
    /// Generally this may be called many times in parallel if many separate
    /// graph executions are required.
    ///
    /// Cancellation model:
    /// - Operations that take >=1 inputs MUST terminate shortly after all input
    ///   streams end/close.
    /// - Operations that only have outputs SHOULD stop themselves when the
    ///   output stream is closed (though this isn't necessary for graph
    ///   correctness).
    /// - When the final graph output streams/nodes end (or the caller loses
    ///   interest in them), all operations will abruptly stop getting polled
    ///   (there is no graceful shutdown).
    async fn execute(&self, inputs: Vec<InputStream>, outputs: Vec<OutputStream>) -> Result<()>;
}
