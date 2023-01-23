# CLI Argument Parsing

This is a library for parsing CLI arguments provided by a user. The format of arguments is mostly aligned with the Abseil flags library and looks like the following:

- String like arguments are passed like: `--flag_name=value`
- Boolean like arguments are passed like `--flag_name`, `--flag_name=true`, or `--flag_name=false`
- Positional arguments are arguments passed like `flag_name` (without a `--` prefix) and may appear anywhere in the list of arguments although should normaly be provided before named flags.
- Escaped arguments are arguments passed after a `--` argument and don't count as positional arguments.

The implementation defines the `ArgType` trait as a Rust type that can be parsed from a single boolean or string flag value. For example, `base_args::list::CommaSeparated` is provided as a way to parse a list of strings.

There is also an `ArgsType` trait for implementing types which can be parsed from multiple arguments although a user should normally not need to use this directly. Instead a convenient macro is provided (see below).

## Usage

Normally a program will define a struct of arguments like follows in order to retrieve arguments:

```rust
#[macro_use]
extern crate macros;

#[derive(Args)]
struct Args {
    file: String,
    write: bool,
    nested: ArgsInner
}

#[derive(Args)]
struct ArgsInner {
    output_file: String
}

fn main() -> Result<()> {
    // 
    let args /*: Args*/ = base_args::parse_args::<Args>()?;
    
    // Use arguments here.
    
    Ok(())
}

```

In the above, a user may provide values for the flags `--file`, `--write`, and `--output_file`.

## Commands

A common usage pattern for CLI interfaces is to support multiple commands via a positional argument. This is supported via the derive macro as follows:

```rust

#[derive(Args)]
struct Args {
    #[arg(positional)]
    file_type: String,
    command: Command
}

#[derive(Args)]
enum Command {
    #[arg(name = "write")]
    Write {
        output_file: String
    },
    #[arg(name = "read")]
    Read {
        input_file: String
    }

}

// ...
```

With the above `Args` struct, a user may run the program in one of the following non-exhaustive ways:

- `./bin zip write --output_file=out`
- `./bin png read --input_file=image.png`

As with the previous example, the enums/structs can be arbitrarily nested to struct shared or command specific arguments.
