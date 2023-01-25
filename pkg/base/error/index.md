# Errors

This library holds the main `Error` type used by all other packages in this project and related utilities.

We have a few key requirements for how we define errorS:

- Support no-alloc or no-std environments (possibly with degraded features).
- Support serialization of errors (either over RPC, to a log, or to string to display to a human).
- Track information such as the retryability of errors.

Design Principles

- All errors are defined as being within an error space (either a single enum or struct)
- Errors shouldn't be returned by APIs in the 'happy' path.
    - Most requests should instantiate 0 error objects and failed requests should produce just 1 error instance.
    - Errors require heap allocation on 'alloc' platforms so are relatively expensive.
- Error types should not contain references (should own all information about the failure).
    - This simplifies the serialization story.
- When will we wrap internal errors
    - APIs will lower level errors into higher level known failure modes (e.g. file not found), but APIs shouldn't aim to wrap everything (e.g. random network failures)
    - Input validation should be wrapped as we care about whether or not errors are retryable with the same inputs.
    - Applications may wrap errors in additional context information to make it easier to debug/trace them.
- APIs that recursively call their own APIs may need to wrap the errors to ensure that the final error is representative of the user's intent.

Implementtion Details

- All errors must implement Display and Debug.
- We will use a #[error] macro which will implement things.
    - Debug will be implemented using the regular 'derive(Debug)'

```rust
// TODO: Have this make enums non-exhaustive?
#[error]
enum MyErrorKind {
    #[desc()]
    NotFound { path: String },




}
```
