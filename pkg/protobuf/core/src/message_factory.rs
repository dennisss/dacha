use std::collections::HashMap;

use alloc::boxed::Box;

use crate::{MessageReflection, StaticMessage};

pub trait MessageFactory: 'static + Send + Sync {
    fn new_message(&self, type_url: &str) -> Option<Box<dyn MessageReflection>>;
}

#[derive(Default)]
pub struct StaticManualMessageFactory {
    types: HashMap<&'static str, fn() -> Box<dyn MessageReflection>>,
}

impl StaticManualMessageFactory {
    pub fn add<T: StaticMessage>(&mut self) -> &mut Self {
        self.types
            .insert(T::static_type_url(), || Box::new(T::default()));
        self
    }
}

impl MessageFactory for StaticManualMessageFactory {
    fn new_message(&self, type_url: &str) -> Option<Box<dyn MessageReflection>> {
        self.types.get(type_url).map(|v| v())
    }
}
