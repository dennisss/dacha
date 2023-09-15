use crate::proto::*;

#[derive(Clone)]
pub struct ConfigurationPending {
    /// Index of the last entry in our log that changes the config
    pub last_change: LogIndex,

    /// Configuration as it was before the last change
    /// In other words the last_applied of this configuration would be
    /// 'last_change - 1'
    pub previous: Configuration,
}

/// Maintains the in-memory state of the configuration with the ability to roll
/// back to the last comitted value of it in the case of log truncations
pub struct ConfigurationStateMachine {
    /// The active version of the configuration that should currently be used.
    pub value: Configuration,

    /// Index of the last log entry applied to this configuration
    /// This should always converge to be exactly the same as the last index in
    /// the log but may start out higher than it if the log has fewer entries
    /// than have been snapshotted
    pub last_applied: LogIndex,

    /// If the current configuration is not yet commited, then this will mark
    /// the last change available
    /// This will allow for rolling back the configuration in case there is a
    /// log conflict
    pub pending: Option<ConfigurationPending>,
}

impl ConfigurationStateMachine {
    pub fn from(snapshot: ConfigurationSnapshot) -> Self {
        ConfigurationStateMachine {
            value: snapshot.data().clone(),
            last_applied: snapshot.last_applied(), // Noteably last_applied must
            pending: None,
        }
    }

    /// Applies the effect of a log entry to the configuration
    /// NOTE: Configuration changes always take immediate effect as soon as they
    /// are in the log
    pub fn apply(&mut self, entry: &LogEntry, commit_index: LogIndex) {
        // Ignore changes when the log is behind our snapshot
        if entry.pos().index() < self.last_applied {
            return;
        }

        if let LogEntryDataTypeCase::Config(change) = entry.data().typ_case() {
            // Only store a revert record if the change is not comitted
            if entry.pos().index() < commit_index {
                self.pending = Some(ConfigurationPending {
                    last_change: entry.pos().index(),
                    previous: self.value.clone(),
                });
            }

            self.value.apply(change.as_ref());
        } else {
            // Other types of entries have no effect on the configuration
        }

        self.last_applied = entry.pos().index();
    }

    /// Given the new end of the log, this will undo any config to the
    /// configuration that occured after that point
    pub fn revert(&mut self, index: LogIndex) {
        if let Some(ref pending) = self.pending.clone() {
            if pending.last_change <= index {
                self.value = pending.previous.clone();
                self.pending = None;
            }
        } else if self.last_applied > index {
            panic!("Attempting to revert from a committed config");
        }

        self.last_applied = index;
    }

    /// Should be called whenever the commit_index has changed
    /// Returns whether or not that had any effect on the latest commit snapshot
    /// available
    pub fn commit(&mut self, commit_index: LogIndex) -> bool {
        let mut changed = false;

        self.pending = match self.pending.take() {
            Some(pending) => {
                // If we committed the entry for the last config change, then we persist the
                // config
                if pending.last_change <= commit_index {
                    changed = true;
                    None
                }
                // Otherwise it is still pending
                else {
                    Some(pending)
                }
            }
            v => v,
        };

        changed
    }

    /// Retrieves the latest persistable version of the configuration
    pub fn snapshot(&self) -> ConfigurationSnapshotRef {
        if let Some(ref pending) = self.pending {
            ConfigurationSnapshotRef {
                last_applied: pending.last_change - 1, /* < Issue being that this applies needing
                                                        * to store multiple indexes in the case
                                                        * of (therefore we must explicitly store
                                                        * the last position in order for this to
                                                        * work properly (or store a reference to
                                                        * the log)) */
                data: &pending.previous,
            }
        } else {
            ConfigurationSnapshotRef {
                last_applied: self.last_applied,
                data: &self.value,
            }
        }
    }
}
