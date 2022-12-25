use alloc::string::String;
use alloc::vec::Vec;

use common::errors::*;

use crate::dns::message::Message;

pub struct MessageCell {
    owned: Vec<u8>,
    value: Message<'static>,
}

impl MessageCell {
    pub fn new<'a, F: Fn(&'a [u8]) -> Result<Message<'a>>>(
        owned: Vec<u8>,
        ctor: F,
    ) -> Result<Self> {
        let value = unsafe {
            let owned_ref = std::mem::transmute::<_, &'static [u8]>(&owned[..]);
            core::mem::transmute(ctor(&owned_ref)?)
        };
        Ok(Self { owned, value })
    }

    pub fn get<'a>(&'a self) -> &'a Message<'a> {
        &self.value
    }
}
