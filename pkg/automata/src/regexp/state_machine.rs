use std::collections::HashSet;

use crate::fsm::*;
use crate::regexp::symbol::*;

pub type RegExpStateMachine = FiniteStateMachine<RegExpSymbol, RegExpEvent, HashSet<RegExpEvent>>;

#[derive(Clone, PartialEq, Hash, Eq, Debug)]
pub enum RegExpEvent {
    StartMatch,
    StartGroup(usize),
    EndGroup(usize),
    // NaN, // InGroup(usize),
}
