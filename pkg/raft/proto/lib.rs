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
    pub fn server(&self, server_id: &ServerId) -> Option<&Configuration_Server> {
        for s in self.servers() {
            if &s.id() == server_id {
                return Some(s);
            }
        }

        None
    }

    pub fn server_role(&self, server_id: &ServerId) -> Configuration_ServerRole {
        self.server(server_id)
            .map(|s| s.role())
            .unwrap_or(Configuration_ServerRole::UNKNOWN)
    }

    fn server_mut(&mut self, server_id: &ServerId) -> &mut Configuration_Server {
        let mut idx = None;
        for (i, s) in self.servers().iter().enumerate() {
            if &s.id() == server_id {
                idx = Some(i);
                break;
            }
        }

        if idx.is_none() {
            let mut s = Configuration_Server::default();
            s.set_id(server_id.clone());
            self.add_servers(s);
            idx = Some(self.servers().len() - 1);
        }

        self.servers_mut()[idx.unwrap()].as_mut()
    }

    // TODO: Move this into a separate ConfigurationStateMachine struct?
    pub fn apply(&mut self, change: &ConfigChange) {
        match change.typ_case() {
            ConfigChangeTypeCase::AddLearner(s) => {
                let server = self.server_mut(s);
                server.set_role(Configuration_ServerRole::LEARNER);
            }
            ConfigChangeTypeCase::AddMember(s) => {
                let server = self.server_mut(s);
                server.set_role(Configuration_ServerRole::MEMBER);
            }
            ConfigChangeTypeCase::AddAspiring(s) => {
                let server = self.server_mut(s);

                match server.role() {
                    Configuration_ServerRole::UNKNOWN
                    | Configuration_ServerRole::ASPIRING
                    | Configuration_ServerRole::LEARNER => {
                        server.set_role(Configuration_ServerRole::ASPIRING);
                    }
                    Configuration_ServerRole::MEMBER => {
                        // Already a member, so no need to downgrade.
                    }
                }
            }
            ConfigChangeTypeCase::RemoveServer(s) => {
                for (i, server) in self.servers().iter().enumerate() {
                    if &server.id() == s {
                        self.servers_mut().remove(i);
                        break;
                    }
                }
            }
            ConfigChangeTypeCase::NOT_SET => {
                // TODO: Return an error.
            }
        };
    }

    // pub fn iter(&self) -> impl Iterator<Item = &ServerId> {
    //     self.members().iter().chain(self.learners().iter())
    // }
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
