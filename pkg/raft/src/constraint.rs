use super::log::*;
use std::sync::Arc;


/// Represents the current state of a constraint retrieved by polling the constraint
pub enum ConstraintPoll<C, T> {
	/// The constraint has been satisfied. The wrapped value is encapsulated in this enum
	Satisfied(T),

	/// The constraint is stll unsatisfied. The constraint is given back to be polled in the future
	Pending(C),

	/// Means that the constraint will never be satisfied therefore the internal data can never be accessed
	Unsatisfiable
}

/// This is a wrapper around some value which optionally enforces that that the inner value cannot be accessed until the log has persisted at least up to the given index
pub struct MatchConstraint<T> {
	inner: T,
	index: Option<LogPosition>
}

impl<T> MatchConstraint<T> {
	pub fn new(inner: T, pos: LogPosition) -> Self {
		MatchConstraint {
			inner, index: Some(pos)
		}
	}

	pub fn poll(self, log: &LogStorage) -> ConstraintPoll<(Self, LogPosition), T> {
		match self.index {
			Some(pos) => {
				match log.term(pos.index) {
					Some(v) => {
						if v != pos.term {
							// Index has been overridden in a newer term
							ConstraintPoll::Unsatisfiable
						}
						else {
							if log.match_index().unwrap_or(0) >= pos.index {
								ConstraintPoll::Satisfied(self.inner)
							}
							else {
								// Not ready yet, reconstruct 'self' and expose the position to the poller 
								let pos_out = pos.clone();
								ConstraintPoll::Pending((
									MatchConstraint {
										inner: self.inner, index: Some(pos)
									},
									pos_out
								))
							}
						}
					},
					// This index has been truncated from the log
					None => ConstraintPoll::Unsatisfiable
				}
			},
			None => ConstraintPoll::Satisfied(self.inner)
		}
	}

}

impl<T> From<T> for MatchConstraint<T> {

	/// Simpler helper for making a completely unconstrained constraint using .into()
    fn from(val: T) -> Self {
        MatchConstraint {
			inner: val,
			index: None
		}
    }
}
