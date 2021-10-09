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
    include!(concat!(env!("OUT_DIR"), "/src/proto/routing.rs"));
}

pub mod key_value {
    include!(concat!(env!("OUT_DIR"), "/src/proto/key_value.rs"));
}

pub mod log {
    include!(concat!(env!("OUT_DIR"), "/src/proto/log.rs"));
}
