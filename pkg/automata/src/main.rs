#[macro_use] extern crate nom;

extern crate core;



mod fsm;
mod regexp;

use fsm::*;
use regexp::*;


fn main() {


	// These three are basically a test case for equivalent languages that can produce different automata

	let mut a = RegExp::parse("a(X|Y)c").unwrap().to_automata().compute_dfa();
	println!("A: {:?}", a);

	let mut b = RegExp::parse("(aXc)|(aYc)").unwrap().to_automata().compute_dfa();
	println!("B: {:?}", b);

	let mut c = RegExp::parse("a(Xc|Yc)").unwrap().to_automata().compute_dfa();
	println!("C: {:?}", c);



	// UnknownSymbol behavior

	let mut rfsm = RegExp::parse("(0|1|2)*01012").unwrap().to_automata();
	rfsm = rfsm.compute_dfa();
	println!("{:?}", rfsm);

	println!("{}", rfsm.accepts("01012".chars()));
	println!("{}", rfsm.accepts("0101012".chars()));
	println!("{}", rfsm.accepts("2220101012".chars()));
	println!("{}", rfsm.accepts("".chars()));

	let mut fsm = FiniteStateMachine::new();

	let s1 = fsm.add_state(); fsm.mark_start(s1);
	let s2 = fsm.add_state();
	let s3 = fsm.add_state();
	let s4 = fsm.add_state();
	let s5 = fsm.add_state();
	let s6 = fsm.add_state(); fsm.mark_accept(s6);

	// Consume any prefix
	fsm.add_transition(s1, '0', s1);
	fsm.add_transition(s1, '1', s1);
	fsm.add_transition(s1, '2', s1);

	fsm.add_transition(s1, '0', s2);
	fsm.add_transition(s2, '1', s3);
	fsm.add_transition(s3, '0', s4);
	fsm.add_transition(s4, '1', s5);
	fsm.add_transition(s5, '2', s6);

	let input = vec![ '0', '1', '0', '1', '0', '1', '2' ];

	let out = fsm.compute_dfa();

	println!("{:?}", out);

	println!("{}", out == rfsm);

	// Composition of state machines:
	// Ideally if we enable state machines to have larger id ranges, then we can implement a method of 

	// Given a well known alphabet, we can check if it detemrministic
	// Also the consideration that if any transitions are not possible, we can define a rejecting state



	// TODO:

	let mut crlf = RegExp::parse(".*abab").unwrap().to_automata().compute_dfa();
	crlf = crlf.compute_dfa();

	println!("HELLO: {}", crlf.accepts("hello world abab".chars()));
}


