# Rust Style Guide

## Whitespace / Spacing

- Use 4 spaces for each indentation level.
- Limit lines to 100 characters

## Errors

Client facing APIs should always prefer to return a `Result<T, base_error::Error>` type rather than a specialized error type.

Specialized error types may still be documented and accessible via downcasting the generic error type.

## Crate Organization

- Crates should be relatively small and low on dependencies.
- Core functionality should be split off into a separate crate compared to advanced functionality if the advanced functionality requires additional high level dependencies.
    - e.g. We have a core `rpc` crate for explicitly the RPC protocol with few dependencies but we have split off utilities into `rpc_utils` which has additional dependencies on components like the `container` crate. In this case, we'd also run into a cyclic dependency if we didn't split these apart.

## Naming

When naming symbols like structs/traits that are being exported from a crate, prefer to avoid prefixing things with the name of the crate. 

For example, in the `http` crate, the HTTP client is referenced as `http::Client` rather than `http::HttpClient`.

## Imports

In a single module file, sort import/'use' statements into three groups separated by blank lines. The three groups should be the following (in this order):

1. Builtin (std/core/alloc crate) depedencies
2. Other crate dependencies
3. Current `crate` dependencies.

Usage of `super` is disallowed aside from as the first statement in a `mod tests` block to do `use super::*;`

For example, well structured imports would look like:

```rust
use std::collections::{HashMap, HashSet};

use http::Client;
use rpc::Server;

use crate::utils::rewrite_file;
```

Also DO NOT use multi-level imports like:

```rust
// AVOID
use protobuf::{text::ParseTextProto, StaticMessage};
```

Instead prefer to split up the imports. e.g. like the following:

```rust
use protobuf::text::ParseTextProto;
use protobuf::StaticMessage;
```


TODO: Naming things Config vs Options