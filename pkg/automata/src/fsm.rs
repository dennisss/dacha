use common::algorithms::DisjointSets;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::default::Default;
use std::hash::Hash;
use std::iter::Extend;
use std::ops::Bound::Included;

/*
    TODO: Optimizations:
    -> If we observe a state chain, we ideally want to be able to flatten any state chain into an optimized comparison function over


    // TODO: Another optimization is to prune transitions back to the start state (we will need to add support for that in the compute_dfa code as well to correctly include that edge on missing symbols)

    So yes, we will simply assume that epsilon is a fake symbol
    -> If all transitions aer just characters, then this would be trivial to maintain

*/

/// Identifier for a single state. This will cap the maximum number of allowable
/// states
pub type StateId = usize;

// TODO: We could implement ordering based on a Hash function as we really don't
// care about complete ordering of symbols
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol<S> {
    Value(S),
    Epsilon,
}

// use std::iter::FromIterator;

pub trait OutputSymbol {
    fn merge(&mut self, other: &Self);
    // fn intersect(&mut self, other: &Self);
}

impl OutputSymbol for () {
    fn merge(&mut self, other: &Self) {}
    // fn intersect(&mut self, other: &Self) {}
}

impl<T: Eq + Hash + Clone> OutputSymbol for HashSet<T> {
    fn merge(&mut self, other: &Self) {
        self.extend(other.iter().cloned())
    }
    // fn intersect(&mut self, other: &Self) {
    //     *self = HashSet::from_iter(self.intersection(other).cloned());
    // }
}

#[derive(Clone, PartialEq, Debug)]
pub struct FiniteStateMachine<S, T: Eq + Hash = (), O: Default + OutputSymbol = ()> {
    /// All states will have ids 0 to num_states
    num_states: StateId,

    // TODO: Use templating to remove this if a user doesn't need tags.
    /// For each state, this will be set of user specified tags associated with
    /// each one. If the automata is transformed, then new states will be tagged
    /// with all tags from original states that derived a state.
    ///
    /// The main purpose of this is to allow tracking an absolute position in
    /// the state machine.
    state_tags: Vec<HashSet<T>>,

    /// The id of the state in which we should be starting
    starting_states: HashSet<StateId>,

    /// Ids of all accepting states
    accepting_states: HashSet<StateId>,

    /// All defined transitionswith the possibility of having multiple
    /// transitions from a single node for a single symbol if the automata is an
    /// NFA or zero for sparse representations
    /// TODO: Could probably be faster using vectors sized by the number of
    /// known states
    transitions: BTreeMap<(StateId, Symbol<S>, StateId), O>,
}

impl<
        S: 'static + Clone + std::cmp::Eq + std::cmp::Ord + std::hash::Hash + std::fmt::Debug,
        T: Eq + Hash + std::fmt::Debug,
        O: 'static + Default + OutputSymbol + std::fmt::Debug,
    > FiniteStateMachine<S, T, O>
{
    pub fn new() -> Self {
        FiniteStateMachine {
            num_states: 0,
            state_tags: vec![],
            starting_states: HashSet::new(),
            accepting_states: HashSet::new(),
            transitions: BTreeMap::new(),
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
        self.state_tags.push(HashSet::new());
        id
    }

    pub fn add_tag(&mut self, state_id: StateId, tag: T) {
        self.state_tags[state_id].insert(tag);
    }

    /// TODO: Instead allow returning an iterator over all state ids.
    pub fn num_states(&self) -> usize {
        self.num_states
    }

    pub fn starts(&self) -> impl Iterator<Item = &StateId> {
        self.starting_states.iter()
    }

    pub fn acceptors(&self) -> impl Iterator<Item = &StateId> {
        self.accepting_states.iter()
    }

    pub fn is_accepting_state(&self, state_id: StateId) -> bool {
        self.accepting_states.contains(&state_id)
    }

    pub fn mark_start(&mut self, id: StateId) {
        self.starting_states.insert(id);
    }

    pub fn mark_accept(&mut self, id: StateId) {
        self.accepting_states.insert(id);
    }

    pub fn add_transition(&mut self, from_id: StateId, sym: S, to_id: StateId) {
        self.add_transition_transducer(from_id, sym, to_id, O::default());
    }

    pub fn add_transition_transducer(
        &mut self,
        from_id: StateId,
        sym: S,
        to_id: StateId,
        output: O,
    ) {
        // TODO: If the edge already exists we should merge the outputs?
        self.transitions
            .insert((from_id, Symbol::Value(sym), to_id), output);
    }

    pub fn add_epsilon_transducer(&mut self, from_id: StateId, to_id: StateId, output: O) {
        self.transitions
            .insert((from_id, Symbol::Epsilon, to_id), output);
    }

    /// For a single state, gets a list of all states that it will transition to
    /// upon seeing a given symbol
    pub fn lookup(&self, from_id: StateId, sym: &S) -> impl Iterator<Item = &StateId> {
        self.lookup_sym(from_id, Symbol::Value(sym.clone()))
    }

    pub fn lookup_transducer(
        &self,
        from_id: StateId,
        sym: &S,
    ) -> impl Iterator<Item = (&StateId, &O)> {
        self.lookup_sym_transducer(from_id, Symbol::Value(sym.clone()))
    }

    fn lookup_sym_transducer(
        &self,
        from_id: StateId,
        sym: Symbol<S>,
    ) -> impl Iterator<Item = (&StateId, &O)> {
        self.transitions
            .range((
                Included((from_id, sym.clone(), 0)),
                Included((from_id, sym.clone(), StateId::max_value())),
            ))
            .map(|((_, _, to_id), outputs)| (to_id, outputs))
    }

    fn lookup_sym(&self, from_id: StateId, sym: Symbol<S>) -> impl Iterator<Item = &StateId> {
        self.lookup_sym_transducer(from_id, sym)
            .map(|(to_id, _)| to_id)
    }

    /// Adds all states and transitions from another automata to the current one
    /// NOTE: This will apply an offset to all ids in the given automata so
    /// previously obtained ids will no longer be valid
    pub fn join(&mut self, mut other: Self) {
        let offset = self.num_states;

        self.num_states += other.num_states;
        self.state_tags.append(&mut other.state_tags);

        for id in other.starting_states {
            self.starting_states.insert(id + offset);
        }

        for id in other.accepting_states {
            self.accepting_states.insert(id + offset);
        }

        for ((i, s, j), v) in other.transitions {
            self.transitions.insert((i + offset, s, j + offset), v);
        }
    }

    /// Chains the given automata to the current one such that current accepting
    /// states become the start starts for the new automata
    pub fn then(&mut self, mut other: Self) {
        let offset = self.num_states;

        self.num_states += other.num_states;
        self.state_tags.append(&mut other.state_tags);

        // Epsilon transitions between all pairs of self_acceptors and other_starts
        for j in other.starting_states {
            for i in self.accepting_states.iter() {
                self.transitions
                    .insert((*i, Symbol::Epsilon, j + offset), O::default());
            }
        }

        // NOTE: Starting states will stay the same

        // Copy accepting states from other
        self.accepting_states.clear();
        for id in other.accepting_states {
            self.accepting_states.insert(id + offset);
        }

        for ((i, s, j), o) in other.transitions {
            self.transitions.insert((i + offset, s, j + offset), o);
        }
    }

    /// Adds back transitions from all acceptors to start states
    /// This basically modifies a regexp 'A' to become 'A+'
    pub fn then_loop(&mut self) {
        for i in self.accepting_states.iter() {
            for j in self.starting_states.iter() {
                self.transitions
                    .insert((*i, Symbol::Epsilon, *j), O::default());
            }
        }
    }

    /// Converts the automata to one with exactly one starting state
    /// If the automata has no starting states, then it will trivially be
    /// converted to an automata with a new start that never transitions
    ///
    /// NOTE: The output is an NFA
    pub fn with_single_start(mut self) -> Self {
        if self.starting_states.len() == 1 {
            return self;
        }

        let s = self.add_state();

        for si in self.starting_states.clone().iter() {
            self.transitions
                .insert((s, Symbol::Epsilon, *si), O::default());
        }

        self.starting_states.clear();
        self.starting_states.insert(s);

        self
    }

    pub fn has_epsilon(&self) -> bool {
        for (_, s, _) in self.transitions.keys() {
            if let Symbol::Epsilon = s {
                return true;
            }
        }

        false
    }

    /// Gets an iterator of all symbols used in this automata
    /// If all symbols have been used at least once in the graph, then this will
    /// be equivalent to the alphabet of the state machine
    pub fn used_symbols(&self) -> Vec<&S> {
        let mut set = HashSet::new();
        for (_, s, _) in self.transitions.keys() {
            if let Symbol::Value(ref v) = s {
                set.insert(v);
            }
        }

        set.into_iter().collect()
    }

    /// Produces an equivalent NFA from the current NFA with all epsilons
    /// removed.
    /// This functions by combining a state with all other states reachable by
    /// only epsilon transitions
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

        let epsilon_outputs = self.find_epsilon_outputs();

        // How many new states there are
        let mut new_num_states = 0;

        let mut new_state_tags: Vec<HashSet<T>> = vec![];

        // For each old state, this will be the index of the new state for it
        let mut state_mapping = vec![];
        state_mapping.reserve_exact(self.num_states);

        // We will make one new state per closure of many old states
        for (i, tags) in (0..self.num_states).zip(self.state_tags.into_iter()) {
            let c = closures.find_set_min(i);
            if c < i {
                let last_id = state_mapping[c];
                state_mapping.push(last_id);

                for tag in tags {
                    new_state_tags[last_id as usize].insert(tag);
                }
            } else if c == i {
                let id = new_num_states;
                new_num_states += 1;
                new_state_tags.push(tags);
                state_mapping.push(id);
            } else {
                panic!("Finding min set failed");
            }
        }

        let new_accepting_states = self
            .accepting_states
            .iter()
            .map(|s| state_mapping[*s])
            .collect();

        let new_starting_states = self
            .starting_states
            .iter()
            .map(|s| state_mapping[*s])
            .collect();

        let mut new_transitions = BTreeMap::<_, O>::new();
        for ((i, s, j), mut o) in self.transitions.into_iter() {
            if let Symbol::Epsilon = s {
                continue;
            }

            let eo = epsilon_outputs.get(&j).unwrap();
            o.merge(eo);

            // TODO: Is it possible for the key to already exist?
            let key = (state_mapping[i], s, state_mapping[j]);
            if let Some(outputs) = new_transitions.get_mut(&key) {
                outputs.merge(&o);
            } else {
                new_transitions.insert(key, o);
            }
        }

        FiniteStateMachine {
            num_states: new_num_states,
            state_tags: new_state_tags,
            starting_states: new_starting_states,
            accepting_states: new_accepting_states,
            transitions: new_transitions,
        }
    }

    fn find_epsilon_outputs(&self) -> HashMap<StateId, O> {
        let mut epsilon_outputs = HashMap::<StateId, O>::new();

        for i in 0..self.num_states {
            self.find_epsilon_outputs_for_state(i, &mut epsilon_outputs);
        }

        epsilon_outputs
    }

    fn find_epsilon_outputs_for_state<'a>(
        &self,
        s: StateId,
        epsilon_outputs: &'a mut HashMap<StateId, O>,
    ) -> &'a O {
        if !epsilon_outputs.contains_key(&s) {
            let mut outputs = O::default();

            // Mainly to stop loops.
            epsilon_outputs.insert(s, O::default());
            for (next_s, o) in self.lookup_sym_transducer(s, Symbol::Epsilon) {
                outputs.merge(o);
                outputs.merge(self.find_epsilon_outputs_for_state(*next_s, epsilon_outputs));
            }
            epsilon_outputs.insert(s, outputs);
        }

        epsilon_outputs.get(&s).unwrap()
    }

    /// Checks if the given slice is accepted by this automata
    /// Unknown symbols not mentioned in the automata will trigger a rejection
    ///
    /// TODO: Return an error with a special code if an unknown symbol is found.
    ///
    /// NOTE: We assume that the automata has already been converted into a DFA
    pub fn accepts<I>(&self, val: I) -> bool
    where
        I: Iterator<Item = S>,
    {
        // NOTE: The DFA should have exactly one starting state.
        let mut i = match self.starting_states.iter().next() {
            Some(i) => *i,
            None => return false,
        };

        for v in val {
            match self.lookup(i, &v).next() {
                Some(j) => {
                    i = *j;
                }
                None => return false,
            };
        }

        self.accepting_states.contains(&i)
    }

    pub fn tags(&self, state_id: StateId) -> &HashSet<T> {
        &self.state_tags[state_id]
    }

    /// Produces a DFA from the current NFA using the Powerset Construction
    /// method (https://en.wikipedia.org/wiki/Powerset_construction)
    ///
    /// The output will have exactly one transition per known symbol per state.
    /// It is only guranteed to be a true DFA if every single symbol of the
    /// alphabet was used in the original NFA
    ///
    /// NOTE: This may also produce an empty set which essentially functions as
    /// a 'never-accepting' state if we encounter a string which is not
    /// representable by the NFA
    ///
    /// NOTE: This should also have the convenient effect of automatically
    /// removing all unreachable states present in the original automata
    pub fn compute_dfa(mut self) -> Self {
        self = self.with_single_start().without_epsilons();

        // TODO: Basically for anything that has less than 128 states, it would probably
        // be more efficient to use a bit set to represent the sets

        // TODO: Keep track of 'dead' states that just consume everything.

        // TODO: The alphabet can be larger than this, but i don't think it
        // really matters as long as the accept() function is smart enough to
        // handle that.
        let alpha = self.used_symbols();

        let mut new_starting_states = HashSet::new();
        new_starting_states.insert(0);

        let mut new_accepting_states = HashSet::new();

        // All powersets that we have created so far
        // (each one is a sorted list of state ids in the original automata)
        let mut new_states = Vec::<Vec<StateId>>::new();
        let mut new_states_idx = HashMap::<Vec<StateId>, usize>::new();
        let mut new_transitions = BTreeMap::<_, O>::new();

        // Assuming no epsilon transitions, the first state will stay the same (this
        // also assumes that there is exactly one starting state now)
        let initial = vec![*self.starting_states.iter().next().unwrap()];
        new_states.push(initial.clone());
        new_states_idx.insert(initial.clone(), 0);
        // Check if the state we just added to the index is accepting already.
        for idx in initial {
            if self.accepting_states.contains(&idx) {
                new_accepting_states.insert(0);
                break;
            }
        }

        // List fo all unvisited states we have seen in the powerset
        let mut queue = vec![0];

        while let Some(cur_id) = queue.pop() {
            for sym in alpha.iter() {
                // Produce the next set based on all reachable states using this symbol
                let mut next_accepts = false;
                let mut next_outputs = O::default();
                let next_set = {
                    let mut set = BTreeSet::new();

                    let cur_set = &new_states[cur_id];

                    for state in cur_set.iter() {
                        // TODO: We should be able to take ownership of the old outputs here by
                        // deleting them from the map.
                        let edges = self.lookup_transducer(*state, sym);
                        for (e, outputs) in edges {
                            if self.accepting_states.contains(e) {
                                next_accepts = true;
                            }

                            set.insert(*e);
                            next_outputs.merge(outputs);
                        }
                    }

                    // TODO: We should support a mode that performs this optimization to produce
                    // sparse automata
                    /*
                    if set.len() == 0 {
                        continue;
                    }
                    */

                    set.into_iter().collect::<Vec<_>>()
                };

                // Create/get the id of this set
                let next_id = if let Some(id) = new_states_idx.get(&next_set).clone().map(|x| *x) {
                    id
                } else {
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

                // TODO: Can this ever overwrite an existing value?
                let key = (cur_id, Symbol::Value((*sym).clone()), next_id);
                if let Some(o) = new_transitions.get_mut(&key) {
                    o.merge(&next_outputs);
                } else {
                    new_transitions.insert(key, next_outputs);
                }
            }
        }

        let mut new_state_tags = vec![];
        new_state_tags.reserve(new_states.len());
        for new_state in new_states.iter() {
            let mut tags = HashSet::new();
            for s in new_state {
                for t in self.state_tags[*s].drain() {
                    tags.insert(t);
                }
            }
            new_state_tags.push(tags);
        }

        println!("TAGS {:?}", new_state_tags);

        FiniteStateMachine {
            num_states: new_states.len(),
            state_tags: new_state_tags,
            starting_states: new_starting_states,
            accepting_states: new_accepting_states,
            transitions: new_transitions,
        }
    }

    /// Produces an NFA automata that accepts the reverse language as the one
    /// represented by the current one
    pub fn reverse(self) -> Self {
        FiniteStateMachine {
            num_states: self.num_states,
            state_tags: self.state_tags,
            starting_states: self.accepting_states,
            accepting_states: self.starting_states,
            transitions: self
                .transitions
                .into_iter()
                .map(|((i, s, j), o)| ((j, s, i), o))
                .collect(),
        }
    }

    /// Produces an equivalent automata with the minimum possible states
    ///
    /// See https://en.wikipedia.org/wiki/DFA_minimization
    /// Currently uses the Brzozowski's algorithm
    pub fn minimal(self) -> Self {
        // NOTE: The powerset construction should automatically prune all
        // unreachable states
        self.reverse().compute_dfa().reverse().compute_dfa()
    }
}
