
#[macro_use]
extern crate regexp_macros;
extern crate automata;

regexp!(TEST => "(hello)|(world)");

fn main() {
    assert_eq!(TEST.test("hello"), true);

    let input = "hello world";

    let mut m = TEST.exec(input).unwrap();
    assert_eq!(m.group(1), Some(b"hello".as_ref()));
    assert_eq!(m.group(2), None);
    assert_eq!(m.index(), 0);
    assert_eq!(m.last_index(), 5);

    m = m.next().unwrap();
    assert_eq!(m.group(1), None);
    assert_eq!(m.group(2), Some(b"world".as_ref()));
    assert_eq!(m.index(), 6);
    assert_eq!(m.last_index(), 11);

    assert!(m.next().is_none());

    println!("All good!");
}