#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate common;
extern crate protobuf;
#[macro_use]
extern crate macros;

include!(concat!(env!("OUT_DIR"), "/proto_lib.rs"));

use std::str::FromStr;
use std::string::{String, ToString};

use common::errors::*;

use crate::raft::*;

impl LogPosition {
    pub fn new<T: Into<Term>, I: Into<LogIndex>>(term: T, index: I) -> Self {
        let mut inst = Self::default();
        inst.set_term(term);
        inst.set_index(index);
        inst
    }

    /// Gets the zero log position, will always be the starting position
    /// before the first real log entry
    pub fn zero() -> Self {
        let mut pos = Self::default();
        // pos.index_mut().set_value(0);
        // pos.term_mut().set_value(0);
        pos
    }
}

impl Configuration {
    pub fn apply(&mut self, change: &ConfigChange) {
        match change.typ_case() {
            ConfigChangeTypeCase::AddLearner(s) => {
                if self.members().contains(s) {
                    // TODO: Is this pretty much just a special version of
                    // removing a server
                    panic!("Can not change member to learner");
                }

                self.learners_mut().insert(*s);
            }
            ConfigChangeTypeCase::AddMember(s) => {
                self.learners_mut().remove(s);
                self.members_mut().insert(*s);
            }
            ConfigChangeTypeCase::RemoveServer(s) => {
                self.learners_mut().remove(s);
                self.members_mut().remove(s);
            }
            ConfigChangeTypeCase::NOT_SET => {
                // TODO: Return an error.
            }
        };
    }

    pub fn iter(&self) -> impl Iterator<Item = &ServerId> {
        self.members().iter().chain(self.learners().iter())
    }
}

impl GroupId {
    pub fn to_string(&self) -> String {
        self.value().to_string()
    }
}

impl FromStr for GroupId {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let mut id = GroupId::default();
        *id.value_mut() = s.parse()?;
        Ok(id)
    }
}
