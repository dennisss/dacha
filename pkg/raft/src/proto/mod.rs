#![allow(dead_code, non_snake_case, unused_imports)]

pub mod consensus {
    include!(concat!(env!("OUT_DIR"), "/src/proto/consensus.rs"));

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

    impl PartialEq for LogPosition {
        fn eq(&self, other: &Self) -> bool {
            self.term() == other.term() && self.index() == other.index()
        }
    }
}

pub mod consensus_state {
    use super::consensus::*;

    include!(concat!(env!("OUT_DIR"), "/src/proto/consensus_state.rs"));

    impl Configuration {
        pub fn apply(&mut self, change: &ConfigChange) {
            match change.type_case() {
                ConfigChangeTypeCase::AddLearner(s) => {
                    if self.members.contains(s) {
                        // TODO: Is this pretty much just a special version of
                        // removing a server
                        panic!("Can not change member to learner");
                    }

                    self.learners.insert(*s);
                }
                ConfigChangeTypeCase::AddMember(s) => {
                    self.learners.remove(s);
                    self.members.insert(*s);
                }
                ConfigChangeTypeCase::RemoveServer(s) => {
                    self.learners.remove(s);
                    self.members.remove(s);
                }
                ConfigChangeTypeCase::Unknown => {
                    // TODO: Return an error.
                }
            };
        }

        pub fn iter(&self) -> impl Iterator<Item = &ServerId> {
            self.members.iter().chain(self.learners.iter())
        }
    }
    pub struct ConfigurationSnapshotRef<'a> {
        pub last_applied: LogIndex,
        pub data: &'a Configuration,
    }
}

pub mod server_metadata {
    use std::str::FromStr;

    include!(concat!(env!("OUT_DIR"), "/src/proto/server_metadata.rs"));

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
}

pub mod routing {
    use std::str::FromStr;

    use super::consensus::ServerId;

    include!(concat!(env!("OUT_DIR"), "/src/proto/routing.rs"));

    // TODO: Verify that these are never used.
    /*
    impl std::hash::Hash for ServerDescriptor {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.id.hash(state);
        }
    }

    impl PartialEq for ServerDescriptor {
        fn eq(&self, other: &ServerDescriptor) -> bool {
            self.id == other.id
        }
    }
    impl Eq for ServerDescriptor {}
    */

    // Mainly so that we can look up servers directly by id in the hash sets
    impl std::borrow::Borrow<ServerId> for ServerDescriptor {
        fn borrow(&self) -> &ServerId {
            &self.id
        }
    }

    impl ServerDescriptor {
        pub fn to_string(&self) -> String {
            self.id().value().to_string() + " " + &self.addr()
        }
    }

    impl FromStr for ServerDescriptor {
        type Err = Error;
        fn from_str(s: &str) -> Result<Self> {
            let parts = s.split(' ').collect::<Vec<_>>();

            if parts.len() != 2 {
                return Err(err_msg("Wrong number of parts"));
            }

            let id = parts[0].parse().map_err(|_| err_msg("Invalid server id"))?;
            let addr = parts[1].to_owned();

            let mut desc = ServerDescriptor::default();
            *desc.id_mut().value_mut() = id;
            desc.set_addr(addr);
            Ok(desc)
        }
    }
}

pub mod key_value {
    include!(concat!(env!("OUT_DIR"), "/src/proto/key_value.rs"));
}

pub mod log {
    include!(concat!(env!("OUT_DIR"), "/src/proto/log.rs"));
}
