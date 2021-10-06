use crate::log::log::*;
use crate::log::log_metadata::LogSequence;
use crate::proto::consensus::LogPosition;

/// Represents the current state of a constraint retrieved by polling the
/// constraint.
pub enum ConstraintPoll<C, T> {
    /// The constraint has been satisfied. The wrapped value is encapsulated in
    /// this enum
    Satisfied(T),

    /// The constraint is stll unsatisfied. The constraint is given back to be
    /// polled in the future
    Pending(C),

    /// Means that the constraint will never be satisfied therefore the internal
    /// data can never be accessed.
    Unsatisfiable,
}

/// This is a wrapper around some value which optionally enforces that that the
/// inner value cannot be accessed until the log has persisted at least up to
/// the given sequence
/// TODO: We don't really need to store the LogPosition as long as we don't care
/// about whether or not it succeeded vs whether it completed. As long as the
/// sequence is high enough, then it should be OK to release the constraint
pub struct FlushConstraint<T> {
    inner: T,
    point: Option<(LogSequence, LogPosition)>,
}

impl<T> FlushConstraint<T> {
    pub fn new(inner: T, seq: LogSequence, pos: LogPosition) -> Self {
        FlushConstraint {
            inner,
            point: Some((seq, pos)),
        }
    }

    pub async fn poll(self, log: &dyn Log) -> ConstraintPoll<(Self, LogSequence), T> {
        let (seq, pos) = match self.point {
            Some(pos) => pos,
            None => return ConstraintPoll::Satisfied(self.inner),
        };

        match log.term(pos.index()).await {
            Some(v) => {
                if v != pos.term() {
                    // Index has been overridden in a newer term
                    ConstraintPoll::Unsatisfiable
                } else {
                    // Otherwise, We will need to check for a proper match here
                    if log.last_flushed().await >= seq {
                        // log.has_flushed_past(seq).await {
                        ConstraintPoll::Satisfied(self.inner)
                    } else {
                        // Not ready yet, reconstruct 'self' and expose the
                        // position to the poller
                        let seq_out = seq.clone();
                        ConstraintPoll::Pending((
                            FlushConstraint {
                                inner: self.inner,
                                point: Some((seq, pos)),
                            },
                            seq_out,
                        ))
                    }
                }
            }
            // This index has been truncated from the log
            None => ConstraintPoll::Unsatisfiable,
        }
    }
}

impl<T> From<T> for FlushConstraint<T> {
    /// Simpler helper for making a completely unconstrained constraint using
    /// .into()
    fn from(val: T) -> Self {
        FlushConstraint {
            inner: val,
            point: None,
        }
    }
}
