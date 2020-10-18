extern crate bytes;
extern crate common;
#[macro_use]
extern crate parsing;

mod fsm;
mod regexp;

use fsm::*;
use regexp::*;

fn main() {
    // TODO: Ensure that when the regexp starts with '^', we can fail fast (when not
    // in multiline mode).

    // These three are basically a test case for equivalent languages that can
    // produce different automata

    let mut a = RegExp::new("a(X|Y)c").unwrap();
    println!("A: {:?}", a);

    let mut b = RegExp::new("(aXc)|(aYc)").unwrap();
    println!("B: {:?}", b);

    let mut c = RegExp::new("a(Xc|Yc)").unwrap();
    println!("C: {:?}", c);
    assert!(c.test("aXc"));
    assert!(c.test("aYc"));
    assert!(!c.test("a"));
    assert!(!c.test("c"));
    assert!(!c.test("Y"));
    assert!(!c.test("Yc"));
    assert!(!c.test(""));

    let mut d = RegExp::new("a").unwrap();
    println!("{:?}", d);

    assert!(d.test("a"));
    assert!(!d.test("b"));

    // NOTE: This has infinite matches and matches everything
    let mut e = RegExp::new("[a-z0-9]*").unwrap();
    println!("{:?}", e);
    assert!(e.test("a9034343"));
    assert!(e.test(""));

    let mut j = RegExp::new("[a-b]").unwrap();
    assert!(e.test("a"));
    assert!(e.test("b"));
    assert!(!e.test("c"));

    //	assert!(!e.test("<"));

    /*
        Supporting partial matches:
        - Create start node which supports unlimited transitions of all symbols
        - Create end node which supports unlimited transitions of all symbols
            - Good enough to stop early if we are on such a node.
        - Occasionally we will emit '^' and '$' symbols
    */

    return;

    // UnknownSymbol behavior

    /*
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
    */
}
