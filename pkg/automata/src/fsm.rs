use core::algorithms::DisjointSets;
use std::collections::{HashMap, HashSet, BTreeSet};
use std::ops::Bound::{Included};

/*
	TODO: Optimizations:
	-> If we observe a state chain, we ideally want to be able to flatten any state chain into an optimized comparison function over 


	// TODO: Another optimization is to prune transitions back to the start state (we will need to add support for that in the compute_dfa code as well to correctly include that edge on missing symbols)

	So yes, we will simply assume that epsilon is a fake symbol
	-> If all transitions aer just characters, then this would be trivial to maintain 

*/


/// Identifier for a single state. This will cap the maximum number of allowable states
type StateId = usize;

// TODO: We could implement ordering based on a Hash function as we really don't care about complete ordering of symbols
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol<S> {
	Value(S),
	Epsilon
}

#[derive(Clone, PartialEq, Debug)]
pub struct FiniteStateMachine<S> {
	/// All states will have ids 0 to num_states
	num_states: StateId,

	/// The id of the state in which we should be starting
	starting_states: HashSet<StateId>,

	/// Ids of all accepting states
	accepting_states: HashSet<StateId>,

	/// All defined transitionswith the possibility of having multiple transitions from a single node for a single symbol if the automata is an NFA or zero for sparse representations
	/// TODO: Could probably be faster using vectors sized by the number of known states
	transitions: BTreeSet<(StateId, Symbol<S>, StateId)>
}

impl<S: 'static + Clone + std::cmp::Eq + std::cmp::Ord + std::hash::Hash> FiniteStateMachine<S> {

	pub fn new() -> Self {
		FiniteStateMachine {
			num_states: 0,
			starting_states: HashSet::new(),
			accepting_states: HashSet::new(),
			transitions: BTreeSet::new()
		}
	}

	/// Creates an automata that accepts empty strings
	pub fn zero() -> Self {
		let mut a = Self::new();
		let s = a.add_state();
		a.mark_start(s);
		a.mark_accept(s);
		a
	}

	/// Creates and adds a new state to the machine returning its id
	pub fn add_state(&mut self) -> StateId {
		let id = self.num_states;
		self.num_states += 1;
		id
	}

	pub fn starts(&self) -> impl Iterator<Item=&StateId> {
		self.starting_states.iter()
	}

	pub fn acceptors(&self) -> impl Iterator<Item=&StateId> {
		self.accepting_states.iter()
	}

	pub fn mark_start(&mut self, id: StateId) {
		self.starting_states.insert(id);
	}

	pub fn mark_accept(&mut self, id: StateId) {
		self.accepting_states.insert(id);
	}

	pub fn add_transition(&mut self, from_id: StateId, sym: S, to_id: StateId) {
		self.transitions.insert((from_id, Symbol::Value(sym), to_id));
	}

	/// For a single state, gets a list of all states that it will transition to upon seeing a given symbol
	pub fn lookup(&self, from_id: StateId, sym: &S) -> impl Iterator<Item=&StateId> {
		self.lookup_sym(from_id, Symbol::Value(sym.clone()))
	}

	fn lookup_sym(&self, from_id: StateId, sym: Symbol<S>) -> impl Iterator<Item=&StateId> {
		self.transitions
		.range((
			Included((from_id, sym.clone(), 0)),
			Included((from_id, sym.clone(), StateId::max_value()))
		))
		.map(|(_, _, to_id)| {
			to_id
		})
	}

	/// Adds all states and transitions from another automata to the current one
	/// NOTE: This will apply an offset to all ids in the given automata so previously obtained ids will no longer be valid
	pub fn join(&mut self, other: FiniteStateMachine<S>) {
		let offset = self.num_states;
		
		self.num_states += other.num_states;

		for id in other.starting_states {
			self.starting_states.insert(id + offset);
		}

		for id in other.accepting_states {
			self.accepting_states.insert(id + offset);
		}

		for (i, s, j) in other.transitions {
			self.transitions.insert((
				i + offset, s, j + offset
			));
		}
	}

	/// Chains the given automata to the current one such that current accepting states become the start starts for the new automata
	pub fn then(&mut self, other: FiniteStateMachine<S>) {
		let offset = self.num_states;
		
		self.num_states += other.num_states;

		// Epsilon transitions between all pairs of self_acceptors and other_starts
		for j in other.starting_states {
			for i in self.accepting_states.iter() {
				self.transitions.insert((*i, Symbol::Epsilon, j + offset));
			}
		}

		// NOTE: Starting states will stay the same
		
		// Copy accepting states from other
		self.accepting_states.clear();
		for id in other.accepting_states {
			self.accepting_states.insert(id + offset);
		}

		for (i, s, j) in other.transitions {
			self.transitions.insert((
				i + offset, s, j + offset
			));
		}
	}

	/// Adds back transitions from all acceptors to start states
	/// This basically modifies a regexp 'A' to become 'A+'
	pub fn then_loop(&mut self) {
		for i in self.accepting_states.iter() {
			for j in self.starting_states.iter() {
				self.transitions.insert((*i, Symbol::Epsilon, *j));
			}
		}
	}

	/// Converts the automata to one with exactly one starting state
	/// If the automata has no starting states, then it will trivially be converted to an automata with a new start that never transitions
	/// 
	/// NOTE: The output is an NFA
	pub fn with_single_start(mut self) -> Self {

		if self.starting_states.len() == 1 {
			return self;
		}

		let s = self.add_state();

		for si in self.starting_states.clone().iter() {
			self.transitions.insert((s, Symbol::Epsilon, *si));
		}

		self.starting_states.clear();
		self.starting_states.insert(s);

		self
	}

	pub fn has_epsilon(&self) -> bool {
		for (_, s, _) in self.transitions.iter() {
			if let Symbol::Epsilon = s {
				return true;
			}
		}

		false
	}

	/// Gets an iterator of all symbols used in this automata
	/// If all symbols have been used at least once in the graph, then this will be equivalent to the alphabet of the state machine
	pub fn used_symbols(&self) -> Vec<&S> {
		let mut set = HashSet::new();
		for (_, s, _) in self.transitions.iter() {
			if let Symbol::Value(ref v) = s {
				set.insert(v);
			}
		}

		set.into_iter().collect()
	}


	/// Produces an equivalent NFA from the current NFA with all epsilons removes
	/// This functions by combining a state with all other states reachable by only epsilon transitions
	pub fn without_epsilons(self) -> Self {

		// Finding the epsilon in the alphabet
		if !self.has_epsilon() {
			return self;
		}


		// Finding all closures of epsilons
		let mut closures = DisjointSets::new(self.num_states);

		for i in 0..self.num_states {
			for j in self.lookup_sym(i, Symbol::Epsilon) {
				closures.union_sets(i, *j);
			}
		}

		// How many new states there are
		let mut num_new_states = 0;

		// For each old state, this will be the index of the new state for it
		let mut state_mapping = vec![];
		state_mapping.reserve_exact(self.num_states);

		// We will make one new state per closure of many old states
		for i in 0..self.num_states {
			let c = closures.find_set_min(i);
			if c < i {
				let last_id = state_mapping[c];
				state_mapping.push(last_id);	
			}
			else if c == i {
				let id = num_new_states;
				num_new_states += 1;

				state_mapping.push(id);
			}
			else {
				panic!("Finding min set failed");
			}
		}

		let new_accepting_states = self.accepting_states.iter().map(|s| {
			state_mapping[*s]
		}).collect();

		let new_starting_states = self.starting_states.iter().map(|s| {
			state_mapping[*s]
		}).collect();

		let mut new_transitions = BTreeSet::new();
		for (i, s, j) in self.transitions.into_iter() {
			if let Symbol::Epsilon = s {
				continue;
			}

			new_transitions.insert((
				state_mapping[i], s, state_mapping[j]
			));
		}

		FiniteStateMachine {
			num_states: num_new_states,
			starting_states: new_starting_states,
			accepting_states: new_accepting_states,
			transitions: new_transitions
		}
	}

	/// Checks if the given slice is accepted by this automata
	/// Unknown symbols not mentioned in the automata will trigger a rejection
	/// 
	/// NOTE: We assume that the automata has already been converted into a DFA
	pub fn accepts<I>(&mut self, val: I) -> bool where I: Iterator<Item=S> {
		let mut i = match self.starting_states.iter().next() {
			Some(i) => *i,
			None => return false
		};

		for v in val {
			match self.lookup(i, &v).next() {
				Some(j) => { i = *j; }
				None => return false
			};
		}

		self.accepting_states.contains(&i)
	}

	/// Produces a DFA from the current NFA using the Powerset Construction method 
	/// (https://en.wikipedia.org/wiki/Powerset_construction)
	/// 
	/// The output will have exactly one transition per known symbol per state. It is only guranteed to be a true DFA if every single symbol of the alphabet was used in the original NFA
	/// 
	/// NOTE: This may also produce an empty set which essentially functions as a 'never-accepting' state if we encounter a string which is not representable by the NFA
	/// 
	/// NOTE: This should also have the convenient effect of automatically removing all unreachable states present in the original automata
	pub fn compute_dfa(mut self) -> Self {

		self = self.with_single_start().without_epsilons();

		// TODO: Basically for anything that has less than 128 states, it would probably be more efficient to use a bit set to represent the sets

		let alpha = self.used_symbols();

		let mut new_starting_states = HashSet::new();
		new_starting_states.insert(0);

		let mut new_accepting_states = HashSet::new();

		// All powersets that we have created so far
		// (each one is a sorted list of state ids in the original automata)
		let mut new_states = Vec::<Vec<StateId>>::new();
		let mut new_states_idx = HashMap::<Vec<StateId>, usize>::new();
		let mut new_transitions = BTreeSet::new();

		// Assuming no epsilon transitions, the first state will stay the same (this also assumes that there is exactly one starting state now)
		let initial = vec![*self.starting_states.iter().next().unwrap()];
		new_states.push(initial.clone());
		new_states_idx.insert(initial.clone(), 0);

		// List fo all unvisited states we have seen in the powerset
		let mut queue = vec![0];
	
		while let Some(cur_id) = queue.pop() {

			for sym in alpha.iter() {

				// Produce the next set based on all reachable states using this symbol 
				let mut next_accepts = false;
				let next_set = {
					let mut set = BTreeSet::new();

					let cur_set = &new_states[cur_id];

					for state in cur_set.iter() {
						let edges = self.lookup(*state, sym);
						for e in edges {
							if self.accepting_states.contains(e) {
								next_accepts = true;
							}

							set.insert(*e);
						}
					}

					// TODO: We should support a mode that performs this optimization to produce sparse automata
					/*
					if set.len() == 0 {
						continue;
					}
					*/

					set.into_iter().collect::<Vec<_>>()
				};

				// Create/get the id of this set
				let next_id =
					if let Some(id) = new_states_idx.get(&next_set).clone().map(|x| *x) {
						id
					}
					else {
						// If it is new, then add it to the queue for future traversal
						let id = new_states.len();
						new_states.push(next_set.clone());
						new_states_idx.insert(next_set, id);
						if next_accepts {
							new_accepting_states.insert(id);
						}

						queue.push(id);

						id
					};

				new_transitions.insert((cur_id, Symbol::Value((*sym).clone()), next_id));
			}
		}


		FiniteStateMachine {
			num_states: new_states.len(),
			starting_states: new_starting_states,
			accepting_states: new_accepting_states,
			transitions: new_transitions
		}

	}

	/// Produces an NFA automata that accepts the reverse language as the one represented by the current one
	pub fn reverse(self) -> Self {
		FiniteStateMachine {
			num_states: self.num_states,
			starting_states: self.accepting_states,
			accepting_states: self.starting_states,
			transitions: self.transitions.into_iter().map(|(i, s, j)| {
				(j, s, i)
			}).collect()
		}
	}

	/// Produces an equivalent automata with the minimum possible states
	///
	/// See https://en.wikipedia.org/wiki/DFA_minimization
	/// Currently uses the Brzozowski's algorithm
	pub fn minimal(self) -> Self {
		// NOTE: The powerset construction should automatically prune all unreachable states
		self.reverse().compute_dfa().reverse().compute_dfa()
	}

}
