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



pub type RegExpP = Box<RegExp>;

#[derive(Debug)]
pub enum RegExp {
	Alt(Vec<RegExpP>),
	Expr(Vec<RegExpP>),
	Quantified(RegExpP, Quantifier),
	Capture(RegExpP),
	Literal(char),
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

	pub fn to_automata(&self) -> FiniteStateAutomata<char> {

		match self {
			RegExp::Alt(list) => {
				let mut a = FiniteStateAutomata::new();
				for r in list.iter() {
					a.join(r.to_automata());
				}

				a
			}
			RegExp::Expr(list) => {
				let mut a = FiniteStateAutomata::zero();
				for r in list.iter() {
					a.then(r.to_automata());
				}

				a
			},
			RegExp::Quantified(r, q) => {
				let mut a = r.to_automata();

				match q {
					Quantifier::ZeroOrOne => {
						a.join(FiniteStateAutomata::zero());
					},
					Quantifier::MoreThanZero => {
						a.then_loop();
						a.join(FiniteStateAutomata::zero());
					},
					Quantifier::MoreThanOne => {
						a.then_loop();
					}
				}

				a
			},
			RegExp::Capture(r) => r.to_automata(),
			RegExp::Literal(c) => {
				let mut a = FiniteStateAutomata::new();
				let start = a.add_state(); a.mark_start(start);
				let end = a.add_state(); a.mark_accept(end);
				a.add_transition(start, *c, end);
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

/*
named!(character_class<&str, RegExpP>, alt!(
	do_parse!(
		
	)

))
*/

named!(capture<&str, RegExpP>, alt!(
	do_parse!(
		e: delimited!(char!('('), alternation, char!(')')) >>
		(Box::new(RegExp::Capture(e)))
	)
));

named!(atom<&str, RegExpP>, alt!(
	literal |
	capture
));

named!(literal<&str, RegExpP>, do_parse!(
	c: none_of!(&b"[\\^$.|?*+()"[..]) >>
	(Box::new(RegExp::Literal(c)))
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
