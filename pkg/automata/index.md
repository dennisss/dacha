# Automata Related Algorithms

Currently this includes support for:

- In-memory construction, transformation, and simultation of finite state machines (NFA/DFA)
- Regular expression execution

## RegExp

Regular expressions are supported via either finite state machine simulation or a VM based approach (like in RE2).

The recommeded way to use regular expressions from code is using the `regexp!` macro which performs ahead of time compilation of regular expressions at build time. See below for example usage:

```rust

#[macro_use]
extern crate regexp_macros;

regexp!(PATTERN => "a(b|c)d");

fn func() -> Result<()> {
    let input = "...";

    // Iterate over matches.
    let next_match = PATTERN.exec(input);
    while let Some(m) = next_match {
        // Get the whole match
        println!("{:?}", m.group_str(0)); // prints something like 'Some(Ok("abd"))'
        
        // Get an indexed group
        println!("{:?}", m.group_str(1)); // prints something like 'Some(Ok("b"))'

        // Advance to next match
        next_match = m.next();
    }

    Ok(())
}
```

In the above example, the `PATTERN` variable is a `StaticRegExp` instance which has other useful functions like:

- `test(input: &str) -> bool`
- `split(input: &str) -> Iterator<&str>` : Splits the input using matches of the pattern as a delimiter.


# TODOs

Need some optimizations for:

- Character class matching
- Long constant sequences
    - Easiest method is if we want to match 'string1|string2|string3|..'
        - Then can just use one hasher to find a match to many strings.