
Regular expression library based on finite automata:
-> In the case of multiple character sequences, there exists an optimized case

-> Then basically use a hashtable to map things out

-> simplest reason for this would be to support large text code base searches based on regular expressions
-> Or for scanning extremely large data files

-> for constant strings or one of many constant strings, it would be nice to support evaluation to finite automata

'(helloy|happy)' should ideally share a single state
	-> Essentially trivial to make into 'h(ello|app)y'
	-> Issues will arise with ensuring that we properly handle prefixes

	Given the pattern 

	xoxxo


Regex:
	AABAAX
Input:
	AABAABAAX

^ Demontrates how one would 


The simple state machine:
	1 --A--> 2 --A--> 3 --B--> 4 --A--> 5 --A--> 6 --A--> 7 --X--> [8] 


Regex
	01012
Input
	010101


http://ivanzuzak.info/noam/webapps/fsm_simulator/
	(0+1+2)*01012


.* is essentially equivalent to not specifying a '^' at the beginning of 

Bascally trivial to generate the NFA
	-> Then generate the 


Define a list of states
	Each state is a list of p

	For one state, Map<Symbol, List<State>>  <- In the case of a DFA, the list should always have cardinality one

	-> Ideally we would like to be able to store all rules in one blob of memory to improve cache capability
		-> Also as we will usually have only a single output edge (and then separate cases for everything else), the complexity should be reducable to an if() else() case

	-> For a simple string matching, we need a good in

-> Then for subset construction
	-> Take the initial state
	-> Then for each state, find all states discoverable from that state
	-> Most trivial construction is to consider each superset id to be a bitvector
		-> But for now, we can probably just simply charaacterize it as a list of state indexes

	-> compressed bitvector
		-> First bit:
			- 0 if a real set bit follows
			-> Otherwise 1 implies skipping an entire bit
			-> Each leading 1 will represent a single 

	-> Because of the large number of states, it is generally better to accept many things all at once

	-> Assuming that zero is never part of the comparison, we can use 128


-> Eventually we will rescan and detect sequences of 


Efficient character lookup over the unicode space:
- Closed hash table 

Implementation of the kind
	-> Based on the complexity of the thing, so we change the integer size used to store a state id
	-> Optimized representation for an executable DFA
		-> Based on fundamental primitives
		-> Possibly some type of really fancy bitwise trie
	-> Single-character transition

-> Special nodes:
	-> 'Else' <- To mean everything 
	-> Use binary search on a sorted transition table


