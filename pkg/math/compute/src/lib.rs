#![feature(negative_impls, thread_local)]
#![no_std]

extern crate core;
#[macro_use]
extern crate alloc;
#[macro_use]
extern crate std;

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate parsing;

mod graph;
pub mod io;
mod layers;
mod ops;
mod training;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt::Debug;
use core::sync::atomic::AtomicU64;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use common::errors::*;
use math::array::Array;

pub use crate::graph::*;
pub use crate::layers::*;
pub use crate::ops::*;
pub use crate::training::*;

/*

Defining signatures:
-


struct Stream {
    node: Rc<Node>,
    output_name: String
}

I do need Node ids to deduplicate computation (e.g. if two nodes request same thing)
    - Use numeric monotonic ids

Subgraph execution

- If we introduce 'functions', Makes things harder to fuse, but could make the code smaller at the end.


    - How to deduplicate

But, sometimes

While Op:
- Output is the

-


Mainly we need multi-inputs if we have things like function call to support

Should we assume that we only input/output

Each node has a map<InputKey, OutputKey>

*/

/*
Simple case to check is when the inputs and outputs reference the same thing.
*/

/*
Post Optimization:
- Pruning
- Folding identity operations.

- Merging any ConstantOps that reference the same constant value (mainly relevant for small scalars).
- 0*x = 0
- 1*x = 1
- constant folding
- deduplicate chains of the same ops
    - Assumption is that all operations are deterministic (not stateful).
- pruning?

- (x*y)*z == x*(y*z)
    - Similar thing for addition / subtraction

- Pruning unneeded casts.

- Need to normalize addition/multiplication ordering to improve deduplication.
    - Inputs should be sorted in increasing node id / output index order

- General no-op ops
    - Identity
    - Squeeze/ExpandDims/reductions with no axes specified
    - Sum when the input and output shapes are the same

- Broadcast followed by an op like Add that supports broadcasted inputs.

 */

/*
Do we need to support graph traversal?
- Yes, because we may eventually want to be able to perform

We also want to support robust validation during graph construction.

- e.g. checking what value broadcast rules can be used or validatin that dtypes/shapes match


*/
