use super::fsm::*;

/*
	PCRE Style RegExp Parser

	Grammar rules derived from: https://github.com/bkiers/pcre-parser/blob/master/src/main/antlr4/nl/bigo/pcreparser/PCRE.g4
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
	Class(Vec<CharSet>, bool),
	Capture(RegExpP),
	Literal(Char)

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
