use super::fsm::*;

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


#[derive(Debug)]
enum Quantifier {
	ZeroOrOne,
	MoreThanZero,
	MoreThanOne
}

enum GroupType {
	/// Captured and just output based on its index
	Regular,

	/// Captured and output indexed by a name
	Named(String),
	
	/// Implying that the capture group is not maintained in the output
	Ignore
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
enum Char {
	Value(char),
	Wildcard, // .
	Word, Digit, Whitespace, // \w \d \s
	NotWord, NotDigit, NotWhiteSpace // \W \D \S
}

#[derive(Debug)]
enum CharSet {
	Single(Char),
	Range(Char, Char)
}


pub type RegExpP = Box<RegExp>;

#[derive(Debug)]
pub enum RegExp {
	Alt(Vec<RegExpP>),
	Expr(Vec<RegExpP>),
	Quantified(RegExpP, Quantifier),

	// We will most likely replace these with capture groups
	// Simplifying method:
	// For each operation, we will 
	Class(Vec<CharSet>, bool),
	
	Capture(RegExpP),
	Literal(Char)

	// NOTE: 
	//Start, End
}

impl RegExp {
	pub fn parse(s: &str) -> std::result::Result<RegExpP, usize> {
		match parse_regexp(s) {
			Ok((rest, r)) => {
				if rest.len() == 0 {
					Ok(r)
				}
				else {
					Err(s.len() - rest.len())
				}
			},
			Err(_) => Err(0)
		}
	}

	// TODO: We will want to optimize this to have type &CharSet or something or some other pointer so that it can optimize out the Option<S> inside of the FSM code
	pub fn to_automata(&self) -> FiniteStateMachine<char> {

		match self {
			RegExp::Alt(list) => {
				let mut a = FiniteStateMachine::new();
				for r in list.iter() {
					a.join(r.to_automata());
				}

				a
			}
			RegExp::Expr(list) => {
				let mut a = FiniteStateMachine::zero();
				for r in list.iter() {
					a.then(r.to_automata());
				}

				a
			},
			RegExp::Quantified(r, q) => {
				let mut a = r.to_automata();

				match q {
					Quantifier::ZeroOrOne => {
						a.join(FiniteStateMachine::zero());
					},
					Quantifier::MoreThanZero => {
						a.then_loop();
						a.join(FiniteStateMachine::zero());
					},
					Quantifier::MoreThanOne => {
						a.then_loop();
					}
				}

				a
			},

			// TODO: After an automata has been built, we need to go back and
			// split all automata (or merge any indivual characters into a single character class if they all point to the same place)
			RegExp::Class(_, _) => panic!("Classes not supported yet"),

			RegExp::Capture(r) => r.to_automata(),
			RegExp::Literal(c) => {

				// If we get a character class here, then that is fine as we will keep that as a root symbol that we will not probably not need to decimate any more

				let cc = match c {
					Char::Value(v) => v,
					_ => panic!("Unsupported")
				};

				let mut a = FiniteStateMachine::new();
				let start = a.add_state(); a.mark_start(start);
				let end = a.add_state(); a.mark_accept(end);
				a.add_transition(start, *cc, end);
				a
			}
		}
	}

}


named!(parse_regexp<&str, RegExpP>, do_parse!(
	e: complete!(alternation) >> (e)
));

named!(alternation<&str, RegExpP>, do_parse!(
	alts: separated_nonempty_list_complete!(char!('|'), expr) >>
	(Box::new(RegExp::Alt(alts)))
));

named!(expr<&str, RegExpP>, do_parse!(
	els: many0!(element) >>
	(Box::new(RegExp::Expr(els)))
));

named!(element<&str, RegExpP>, alt_complete!(
	do_parse!(
		a: atom >>

		// Trying to parse 0 or 1 quantifiers
		q: alt_complete!(
			opt!(quantifier) |
			value!(None)
		)  >>
		
		(match q {
			Some(q) => {
				Box::new(RegExp::Quantified(a, q))
			},
			None => a
		})
	)
));

named!(quantifier<&str, Quantifier>, alt!(
	do_parse!( char!('?') >> (Quantifier::ZeroOrOne) ) |
	do_parse!( char!('*') >> (Quantifier::MoreThanZero) ) |
	do_parse!( char!('+') >> (Quantifier::MoreThanOne) )
));

// TODO: Strategy for implementing character classes
// If there are other overlapping symbols, 

// TODO: In PCRE, '[]]' would parse as a character class matching the character ']' but for simplity we will require that that ']' be escaped in a character class
named!(character_class<&str, RegExpP>,
	delimited!(char!('['), do_parse!(
		invert: opt!(char!('^')) >>
		inner: many1!(cc_atom) >>
		(Box::new(RegExp::Class(inner, invert.is_some())))
	), char!(']'))
);

named!(capture<&str, RegExpP>, alt!(
	do_parse!(
		e: delimited!(char!('('), alternation, char!(')')) >>
		(Box::new(RegExp::Capture(e)))
	)
));

named!(atom<&str, RegExpP>, alt!(
	map!(shared_atom, |c| Box::new(RegExp::Literal(c))) |
	literal |
	character_class |
	capture
));

named!(cc_atom<&str, CharSet>, alt!(
	do_parse!(
		start: cc_literal >>
		char!('-') >>
		end: cc_literal >>
		(CharSet::Range(start, end))
	) |
	map!(alt!(
		shared_atom |
		cc_literal
	), |c| CharSet::Single(c))
));

// TODO: It seems like it could be better to combine this with the shared_literal class
named!(shared_atom<&str, Char>, alt!(
	map!(tag!("\\w"), |_| Char::Word) |
	map!(tag!("\\d"), |_| Char::Digit) |
	map!(tag!("\\s"), |_| Char::Whitespace) |
	map!(tag!("\\W"), |_| Char::NotWord) |
	map!(tag!("\\D"), |_| Char::NotDigit) |
	map!(tag!("\\S"), |_| Char::NotWhiteSpace)
));

named!(literal<&str, RegExpP>, do_parse!(
	c: alt!(
		shared_literal |
		char!(']')
	) >>
	(Box::new(RegExp::Literal(Char::Value(c))))
));

// TODO: Check this
named!(cc_literal<&str, Char>, do_parse!(
	c: alt!(
		//shared_literal |
		none_of!(&b"]"[..])
	) >>
	(Char::Value(c))
));

named!(shared_literal<&str, char>, alt!(
	none_of!(&b"[]\\^$.|?*+()"[..]) |
	quoted
));

named!(number<&str, usize>, 
	map_res!(take_while!(|c: char| c.is_digit(10)), |s: &str| s.parse::<usize>())
);

named!(name<&str, String>, do_parse!(
	head: take_while_m_n!(1, 1, |c: char|
		c.is_alphabetic() || c == '_'
	) >>
	rest: take_while_m_n!(1, 1, |c: char|
		c.is_alphabetic() || c == '_' || c.is_digit(10)
	) >>
	(String::from(head) + rest)
));


named!(quoted<&str, char>, do_parse!(
	char!('\\') >>
	s: take_while_m_n!(1, 1, |c: char| !c.is_alphanumeric()) >>
	(s.chars().next().unwrap())
));

/*
named!(caret<&str, RegExpP>, do_parse!(
	char!('^') >>
	(Box::new(RegExp::Start))
));
*/



#[cfg(test)]
mod tests {

	use super::*;


	#[test]
	fn parsing_inline() {
		let r = parse_regexp("123");

		println!("{:?}", r);
	}




}
