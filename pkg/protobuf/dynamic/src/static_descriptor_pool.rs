use alloc::boxed::Box;
use core::ops::Deref;
use std::sync::Arc;

use common::errors::*;
use protobuf_core::{StaticFileDescriptor, StaticMessage};

use crate::descriptor_pool::*;

static mut INSTANCE: std::sync::Mutex<Option<Arc<StaticDescriptorPool>>> =
    std::sync::Mutex::new(None);

pub struct StaticDescriptorPool {
    inner: DescriptorPool,
}

impl StaticDescriptorPool {
    pub fn global() -> Arc<StaticDescriptorPool> {
        unsafe {
            let mut guard = INSTANCE.lock().unwrap();
            if guard.is_none() {
                *guard = Some(Arc::new(StaticDescriptorPool {
                    inner: DescriptorPool::new(DescriptorPoolOptions::default()),
                }));
            }

            guard.as_ref().unwrap().clone()
        }
    }

    pub async fn get_descriptor<M: StaticMessage>(&self) -> Result<MessageDescriptor> {
        self.add_static_file_descriptor(M::file_descriptor())
            .await?;
        self.inner
            .find_by_type_url(M::static_type_url())
            .ok_or_else(|| err_msg("Added descriptor but type missing."))
    }

    async fn add_static_file_descriptor(&self, desc: &'static StaticFileDescriptor) -> Result<()> {
        let mut queue = vec![desc];

        // TODO: Optimize out loads if the same static pointer was added in the past.
        while let Some(desc) = queue.pop() {
            self.inner.add_file_descriptor(desc.proto).await?;

            for dep in desc.dependencies.iter().cloned() {
                queue.push(dep);
            }
        }

        Ok(())
    }
}
