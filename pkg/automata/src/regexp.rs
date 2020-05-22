use common::errors::*;
use parsing::*;
// TODO: Refactor parser to operate on &str instead.
use bytes::Bytes;
use super::fsm::*;
use std::ops::Bound;

/*
	PCRE Style RegExp Parser

	Grammar rules derived from: https://github.com/bkiers/pcre-parser/blob/master/src/main/antlr4/nl/bigo/pcreparser/PCRE.g4
*/

/*
	Representing capture groups:
	- The only complication is alternation:
		- We must pretty much record the list of all states in each branch of an alternation
		- Then after minimization, compare all lists of states and figure out how to differentiate the cases
			-> Based on this we should be able to 

	- Computing shortest string:
		-> Basically remove epsilons and compute graph shortest path
		-> If we can check for cycles then we tell whether or not a language is finite or infinite
	
	Optimization:
		- When observing a long sequence of chained characters, we can optimize the problem into:
			- https://en.wikipedia.org/wiki/Knuth%E2%80%93Morris%E2%80%93Pratt_algorithm
			- Basically the same idea as automata but in a rather condensed format

	More ideas on how to implement capture groups:
	- https://stackoverflow.com/questions/28941425/capture-groups-using-dfa-based-linear-time-regular-expressions-possible
*/

/*
	Character classes:
	-> [abcdef]
		-> For now, regular ones can be split 
		-> We must make every single character class totally orthogonal
*/


/*
	We will ideally be able to reduce everything to

	Given a character class such as:
		'[aa]'
		-> Deduplication will be handled by the FSM
		-> But if we have '[ab\w]'
			-> Then we do need to perform splitting of to

*/

// TODO: What would '[\s-\w]' mean

// NOTE: This will likely end up being the token data for our state machine
// NOTE: We will also need to be able to represent inverses in the character sets
// Otherwise, we will need to make it a closed set ranges up to the maximum value
/*
	Given a sorted set of all possible alphabetic characters
	- We can choose exactly one in log(n) time where n is the size of the alphabet observed in the regex

*/

/*
	Given a list of all characters, we will in most cases perform two operations to look up a symbol id for a given symbol:
	-> 

	-> General idea of subset decimation:
		-> Start out with the set of all symbols:
		-> 
	
	-> Future optimizations:
		-> If a node has all or almost all possible symbols as edges out of it, it is likely gonna be more efficient to perform a method of negation comparison rather than inclusion
			-> Mainly to save on memory storing the transition table 


	So we will have two internal types of regexps:
	-> One full ast tree

	-> then another which excludes custom character classes and non-well defined character classes
	-> We will Want to reduce all non-direct value sets to ranges or other types of closures

*/

struct RegExpSymbolIter<'a> {
	char_iter: std::str::Chars<'a>,
	alphabet: &'a RegExpAlphabet
}

impl<'a> std::iter::Iterator for RegExpSymbolIter<'a> {
	type Item = RegExpSymbol;
	fn next(&mut self) -> Option<Self::Item> {
		self.char_iter.next().map(|c| self.alphabet.get(c))
	}
}


#[derive(Debug)]
pub struct RegExp {
	alphabet: RegExpAlphabet,
	state_machine: FiniteStateMachine<RegExpSymbol>
}

impl RegExp {
	pub fn new(expr: &str) -> Result<Self> {
		let tree = RegExpNode::parse(expr)?;

		let mut alpha = RegExpAlphabet::new();
		tree.fill_alphabet(&mut alpha);

		let mut state_machine = FiniteStateMachine::new();
		let first_state = state_machine.add_state();
		state_machine.mark_accept(first_state);
		state_machine.mark_start(first_state);
		for sym in alpha.all_symbols() {
			state_machine.add_transition(first_state, sym, first_state);
		}

		state_machine.then(tree.to_automata_inner(&alpha));

		// TODO: Instead compute the minimal DFA?
		state_machine = state_machine.compute_dfa();


		Ok(Self {
			alphabet: alpha,
			state_machine
		})
	}

	// TODO: Ensure that matching is greedy, and always continues matching until
	// the current match is exhausted.
	pub fn test(&self, value: &str) -> bool {
		let mut iter = RegExpSymbolIter {
			char_iter: value.chars(), alphabet: &self.alphabet };
		self.state_machine.accepts(iter)
	}

}




// TODO: Unused right now
enum GroupType {
	/// Captured and just output based on its index
	Regular,

	/// Captured and output indexed by a name
	Named(String),
	
	/// Implying that the capture group is not maintained in the output
	Ignore
}

/*
	For
*/

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
enum Char {
	/// A single character that must exactly match the next input.
	Value(char),
	/// A character range expression like '[a-z]'.
	/// NOTE: This will never occur in a RegExprNode::Literal
	/// NOTE: We don't allow '[.-\d]'
	Range(char, char),
	Wildcard, // '.'
	Word, Digit, Whitespace, // '\w' '\d' '\s'
	NotWord, NotDigit, NotWhiteSpace // '\W' '\D' '\S'
}

impl Char {
	// NOTE: These symbols may contain a lot of overlap.
	fn raw_symbols(&self) -> Vec<RegExpSymbol> {
		let mut out = vec![];

		match self {
			Char::Value(c) => {
				out.push(RegExpSymbol::Single(*c));
			},
			Char::Range(s, e) => {
				out.push(RegExpSymbol::InclusiveRange(*s, *e));
			},
			// [0-9]
			Char::Digit | Char::NotDigit => {
				out.push(RegExpSymbol::InclusiveRange('0', '9'));
			},
			// [0-9A-Za-z_]
			Char::Word | Char::NotWord => {
				out.push(RegExpSymbol::InclusiveRange('0', '9'));
				out.push(RegExpSymbol::InclusiveRange('A', 'Z'));
				out.push(RegExpSymbol::InclusiveRange('a', 'z'));
				out.push(RegExpSymbol::Single('_'));
			},
			// [\t\n\f\r ]
			Char::Whitespace | Char::NotWhiteSpace => {
				out.push(RegExpSymbol::Single('\t'));
				out.push(RegExpSymbol::Single('\n'));
				out.push(RegExpSymbol::Single('\x0C'));
				out.push(RegExpSymbol::Single('\r'));
				out.push(RegExpSymbol::Single(' '));
			},
			Char::Wildcard => {
				out.push(RegExpSymbol { start: 0, end: std::char::MAX as u32 });
			}
		}

		let invert = match self {
			Char::NotWhiteSpace | Char::NotDigit | Char::NotWord => true,
			_ => false
		};

		if invert {
			out = invert_symbols(out);
		}

		out
	}

	fn fill_alphabet(&self, alpha: &mut RegExpAlphabet) {
		for sym in self.raw_symbols() {
			alpha.insert(sym);
		}
	}
}

fn invert_symbols(syms: Vec<RegExpSymbol>) -> Vec<RegExpSymbol> {
	let mut out = vec![];
	for item in syms {
		if item.start > 0 {
			out.push(RegExpSymbol {
				start: 0, end: item.start });
		}
		if item.end < (std::char::MAX as u32) {
			out.push(RegExpSymbol {
				start: item.end, end: std::char::MAX as u32 })
		}
	}

	out
}

/// Internal representation of a set of values associated with an edge between
/// nodes in the regular expression's state machine.
///
/// All symbols used in the internal state machine will be non-overlapping.
#[derive(Debug, PartialEq, PartialOrd, Clone, Hash, Eq, Ord)]
struct RegExpSymbol {
	start: u32,
	end: u32
}

impl RegExpSymbol {
	fn Single(c: char) -> Self {
		Self { start: c as u32, end: (c as u32) + 1 }
	}
	fn InclusiveRange(s: char, e: char) -> Self {
		Self { start: (s as u32), end: (e as u32) + 1 }
	}
}


/// e.g. The regular expression '(a|[a-z])' will have an alphabet of the
/// following value ranges:
/// [0, 'a'), ['a', 'b'), ['b', 'z'+1), ['z+1', MAX)
/// ^ This will only exist in the intermediate automata format. once a final
/// DFA has been generated, we can re-merge the symbols per source node into
/// the most optimal matching format.
#[derive(Debug)]
struct RegExpAlphabet {
	offsets: std::collections::BTreeSet<u32>
}

impl RegExpAlphabet {
	fn new() -> Self {
		let mut offsets = std::collections::BTreeSet::new();
		offsets.insert(0);
		offsets.insert(std::char::MAX as u32);
		Self { offsets }
	}

	fn insert(&mut self, sym: RegExpSymbol) {
		self.offsets.insert(sym.start);
		self.offsets.insert(sym.end);
	}

	fn all_symbols(&self) -> Vec<RegExpSymbol> {
		let mut out = vec![];
		let mut iter = self.offsets.iter();
		let mut i = *iter.next().unwrap();
		for j in iter {
			out.push(RegExpSymbol { start: i, end: *j });
			i = *j;
		}

		out
	}

	fn get(&self, c: char) -> RegExpSymbol {
		let v = c as u32;

		// Closest offset <= v
		let i = *self.offsets.range((Bound::Unbounded, Bound::Included(v)))
			.rev().next().unwrap();
		// Closest offset > v
		let j = *self.offsets.range((Bound::Excluded(v), Bound::Unbounded))
			.next().unwrap();

		assert!(i <= v && j > v, "{} <= {} < {}", i, v, j);

		RegExpSymbol { start: i, end: j }
	}

	fn decimate(&self, sym: RegExpSymbol) -> Vec<RegExpSymbol> {
		let mut range = self.offsets.range((
			Bound::Included(sym.start), Bound::Included(sym.end)));

		let mut out = vec![];
		let mut i = range.next().unwrap();
		assert_eq!(*i, sym.start);
		for j in range {
			out.push(RegExpSymbol { start: *i, end: *j });
			i = j;
		}
		assert_eq!(*i, sym.end);

		out
	}

	// NOTE: The output of this may have overlapping symbols.
	fn decimate_many(&self, syms: Vec<RegExpSymbol>) -> Vec<RegExpSymbol> {
		let mut out = vec![];
		for sym in syms {
			out.extend_from_slice(&self.decimate(sym));
		}
		out
	}
}





type RegExpNodePtr = Box<RegExpNode>;

#[derive(Debug)]
enum RegExpNode {
	Alt(Vec<RegExpNodePtr>),
	Expr(Vec<RegExpNodePtr>),
	Quantified(RegExpNodePtr, Quantifier),

	// We will most likely replace these with capture groups
	// Simplifying method:
	// For each operation, we will 
	Class(Vec<Char>, bool),
	
	Capture(RegExpNodePtr),
	Literal(Char)

	// NOTE: 
	//Start, End
}

impl RegExpNode {
	pub fn parse(s: &str) -> Result<RegExpNodePtr> {
		let (res, _) = complete(alternation)(Bytes::from(s))?;
		Ok(res)
//		match parse_regexp(s) {
//			Ok((rest, r)) => {
//				if rest.len() == 0 {
//					Ok(r)
//				}
//				else {
//					Err(s.len() - rest.len())
//				}
//			},
//			Err(_) => Err(0)
//		}
	}

	// TODO: We will want to optimize this to have type &CharSet or something or some other pointer so that it can optimize out the Option<S> inside of the FSM code
	pub fn to_automata(&self) -> FiniteStateMachine<RegExpSymbol> {
		let mut alpha = RegExpAlphabet::new();
		self.fill_alphabet(&mut alpha);
		self.to_automata_inner(&alpha)
	}

	fn to_automata_inner(&self, alpha: &RegExpAlphabet) -> FiniteStateMachine<RegExpSymbol> {
		match self {
			Self::Alt(list) => {
				let mut a = FiniteStateMachine::new();
				for r in list.iter() {
					a.join(r.to_automata_inner(alpha));
				}

				a
			}
			Self::Expr(list) => {
				let mut a = FiniteStateMachine::zero();
				for r in list.iter() {
					a.then(r.to_automata_inner(alpha));
				}

				a
			},
			Self::Quantified(r, q) => {
				let mut a = r.to_automata_inner(alpha);

				match q {
					Quantifier::ZeroOrOne => {
						a.join(FiniteStateMachine::zero());
					},
					Quantifier::ZeroOrMore => {
						a.then_loop();
						a.join(FiniteStateMachine::zero());
					},
					Quantifier::OneOrMore => {
						a.then_loop();
					}
				}

				a
			},

			Self::Capture(r) => r.to_automata_inner(alpha),
			// TODO: After an automata has been built, we need to go back and
			// split all automata (or merge any indivual characters into a
			// single character class if they all point to the same place)
			Self::Class(chars, inverted) => {
				let mut syms = vec![];
				for c in chars {
					syms.extend_from_slice(&alpha.decimate_many(c.raw_symbols()));
				}
				if *inverted {
					syms = invert_symbols(syms);
				}

				syms = alpha.decimate_many(syms);

				// TODO: Same as RegExprNode::Literal case below
				let mut a = FiniteStateMachine::new();
				let start = a.add_state(); a.mark_start(start);
				let end = a.add_state(); a.mark_accept(end);
				for sym in syms {
					a.add_transition(start, sym, end);
				}
				a
			},
			Self::Literal(c) => {
				let syms = alpha.decimate_many(c.raw_symbols());

				let mut a = FiniteStateMachine::new();
				let start = a.add_state(); a.mark_start(start);
				let end = a.add_state(); a.mark_accept(end);
				for sym in syms {
					a.add_transition(start, sym, end);
				}
				a
			}
		}
	}

	fn fill_alphabet(&self, alpha: &mut RegExpAlphabet) {
		match self {
			Self::Alt(list) => {
				for r in list.iter() {
					r.fill_alphabet(alpha);
				}
			},
			Self::Capture(inner) => inner.fill_alphabet(alpha),
			Self::Class(items, invert) => {
				for item in items {
					item.fill_alphabet(alpha);
				}
			},
			Self::Literal(c) => {
				c.fill_alphabet(alpha);
			},
			Self::Quantified(e, _) => e.fill_alphabet(alpha),
			Self::Expr(list) => {
				for item in list {
					item.fill_alphabet(alpha);
				}
			}
		}
	}

}

/*
	General grammar is:
		Regexp -> Alternation
		Alternation -> Expr | ( Expr '|' Alternation )
		Expr -> Element Expr | <empty>
		Element -> (Group | CharacterClass | EscapedLiteral | Literal) Repetitions
		Repetitions -> '*' | '?' | '+' | <empty>

		Group -> '(' Regexp ')'

*/


parser!(alternation<RegExpNodePtr> => {
	map(delimited(expr, tag("|")), |alts| Box::new(RegExpNode::Alt(alts)))
});

parser!(expr<RegExpNodePtr> => {
	map(many(element), |els| Box::new(RegExpNode::Expr(els)))
});

// Element -> Atom Quantifier | Atom
parser!(element<RegExpNodePtr> => {
	seq!(c => {
		let a = c.next(atom)?;
		if let Some(q) = c.next(opt(Quantifier::parse))? {
			return Ok(Box::new(RegExpNode::Quantified(a, q)));
		}

		Ok(a)
	})
});

#[derive(Debug)]
enum Quantifier {
	ZeroOrOne,
	ZeroOrMore,
	OneOrMore
}

impl Quantifier {
	parser!(parse<Self> => alt!(
		map(tag("?"), |_| Quantifier::ZeroOrOne),
		map(tag("*"), |_| Quantifier::ZeroOrMore),
		map(tag("+"), |_| Quantifier::OneOrMore)
	));
}

parser!(atom<RegExpNodePtr> => alt!(
	map(shared_atom, |c| Box::new(RegExpNode::Literal(c))),
	literal,
	character_class,
	capture
));


// TODO: Strategy for implementing character classes
// If there are other overlapping symbols, 

// TODO: In PCRE, '[]]' would parse as a character class matching the character ']' but for simplity we will require that that ']' be escaped in a character class
parser!(character_class<RegExpNodePtr> => seq!(c => {
	c.next(tag("["))?;
	let invert = c.next(opt(tag("^")))?;
	let inner = c.next(many(character_class_atom))?; // NOTE: We allow this to be empty.
	c.next(tag("]"))?;

	return Ok(Box::new(RegExpNode::Class(inner, invert.is_some())));
}));

parser!(capture<RegExpNodePtr> => seq!(c => {
	c.next(tag("("))?;
	let inner = c.next(alternation)?;
	c.next(tag(")"))?;

	Ok(Box::new(RegExpNode::Capture(inner)))
}));


parser!(character_class_atom<Char> => alt!(
	seq!(c => {
		let start = c.next(character_class_literal)?;
		c.next(tag("-"))?;
		let end = c.next(character_class_literal)?;

		// TODO: Return this as an error, but don't allow trying to parse
		// other alt! cases.
		assert!(end >= start);

		// In this case,
		Ok(Char::Range(start, end))
	}),

	shared_atom,
	map(character_class_literal, |c| Char::Value(c))
));

// TODO: Ensure that we support \t, \f \a, etc. See:
// https://github.com/google/re2/wiki/Syntax (search 'Escape Sequences')

// TODO: It seems like it could be better to combine this with the shared_literal class
parser!(shared_atom<Char> => alt!(
	map(tag("\\w"), |_| Char::Word),
	map(tag("\\d"), |_| Char::Digit),
	map(tag("\\s"), |_| Char::Whitespace),
	map(tag("\\W"), |_| Char::NotWord),
	map(tag("\\D"), |_| Char::NotDigit),
	map(tag("\\S"), |_| Char::NotWhiteSpace)
));

// A single plain character that must be exactly matched.
// This rule does not apply to anything inside a character class.
// e.g. the regexp 'ab' contains 2 literals.
parser!(literal<RegExpNodePtr> => {
	map(alt!(shared_literal, map(tag("]"), |v| v[0] as char)),
		|c| Box::new(RegExpNode::Literal(Char::Value(c))))
});

// TODO: Check this
parser!(character_class_literal<char> => {
	//shared_literal |
	map(not_one_of("]"), |c| c as char)
});

// Single characters which need to be matched exactly
// (excluding symbols which may have a different meaning depending on context)
parser!(shared_literal<char> => alt!(
	map(not_one_of("[]\\^$.|?*+()"), |v| v as char),
	quoted
));

//named!(number<&str, usize>,
//	map_res!(take_while!(|c: char| c.is_digit(10)), |s: &str| s.parse::<usize>())
//);

//named!(name<&str, String>, do_parse!(
//	head: take_while_m_n!(1, 1, |c: char|
//		c.is_alphabetic() || c == '_'
//	) >>
//	rest: take_while_m_n!(1, 1, |c: char|
//		c.is_alphabetic() || c == '_' || c.is_digit(10)
//	) >>
//	(String::from(head) + rest)
//));

// Matches '\' followed by the character being escaped.
parser!(quoted<char> => {
	seq!(c => {
		c.next(tag("\\"))?;
		let v = c.next(take_exact(1))?[0] as char;
		if v.is_alphanumeric() {
			return Err(err_msg("Expected non alphanumeric character"));
		}

		Ok(v)
	})
});


#[cfg(test)]
mod tests {

	use super::*;


	#[test]
	fn parsing_inline() {
		let r = parse_regexp("123");

		println!("{:?}", r);
	}




}
