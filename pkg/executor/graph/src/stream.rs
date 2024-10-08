use std::collections::HashMap;
use std::sync::Arc;
use std::{any::Any, collections::HashSet};

use base_error::*;
use executor::bundle::TaskResultBundle;
use executor::channel::spsc::{self, Receiver, Sender};

// Getting this error means that that the other end of this stream is dead and
// never finished fully reading/writing to the end of the stream.
#[derive(Debug, Fail, Clone, Copy)]
pub struct GraphStreamError {
    //
}

impl core::fmt::Display for GraphStreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GraphStreamError")
    }
}

pub struct InputStream {
    pub(crate) receiver: Receiver<Option<Box<dyn Any + Send + Sync + 'static>>>,
}

impl InputStream {
    /// TODO: Need to differentiate the end of inputs and the stream being fully
    /// consumed.
    pub async fn read(
        &mut self,
    ) -> Result<Option<Box<dyn Any + Send + Sync + 'static>>, GraphStreamError> {
        match self.receiver.recv().await {
            Ok(v) => Ok(v),
            Err(_) => Err(GraphStreamError {}),
        }
    }
}

pub struct OutputStream {
    pub(crate) sender: Sender<Option<Box<dyn Any + Send + Sync + 'static>>>,
}

impl OutputStream {
    /// Writes a single packet/frame of data to the stream. This will block if
    /// the unprocessed packet queue is full.
    ///
    /// Returns false if the other end of the stream has been dropped.
    pub async fn write(
        &mut self,
        data: Box<dyn Any + Send + Sync + 'static>,
    ) -> Result<(), GraphStreamError> {
        if self.sender.send(Some(data)).await.is_err() {
            return Err(GraphStreamError {});
        }

        Ok(())
    }

    pub async fn close(&mut self) {
        let _ = self.sender.send(None).await;
    }
}
