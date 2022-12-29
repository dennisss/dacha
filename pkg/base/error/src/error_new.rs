/*
Error strategy:
- Requirements:
    - Support no-alloc or no-std environments (possibly with degraded features).
    - Support serialization of errors (either over RPC, to a log, or to string to display to a human).
    - Track information such as the retryability of errors.

- Design Principles
    - All errors are defined as being within an error space (either a single enum or struct)
    - Errors shouldn't be used in the happen path.
        - Most requests should instantiate 0 error objects and failed requests should produce just 1 error instance.
        - Errors require heap allocation on 'alloc' platforms so are relatively expensive.
    - Error types should not contain references (should own all information about the failure).
        - This simplifies the serialization story.
    - When will we wrap internal errors
        - APIs will lower level errors into higher level known failure modes (e.g. file not found), but APIs shouldn't aim to wrap everything (e.g. random network failures)
        - Input validation should be wrapped as we care about whether or not errors are retryable with the same inputs.
        - Applications may wrap errors in additional context information to make it easier to debug/trace them.
    - APIs that recursively call their own APIs may need to wrap the errors to ensure that the final

- Implementtion Details
    - All errors must implement Display and Debug.
    - We will use a #[error] macro which will implement things.
        - Debug will be implemented using the regular 'derive(Debug)'
    -

// TODO: Have this make enums non-exhaustive?
#[error]
enum MyErrorKind {
    #[desc()]
    NotFound { path: String },




}


Types to deal with:
- Error
    - This is a struct

- Errable
    - A trait for something that can be converted into an Error (and required to have some basic traits).




- std::error::Error
    - This is a trait

*/

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
use core::any::{Any, TypeId};
use core::convert::From;
use core::convert::Infallible;
use core::fmt::{Debug, Display};

// Things I need:
// - Mainly for debugging,

#[cfg(feature = "alloc")]
pub struct Error {
    inner: Box<dyn Errable>,
}

#[cfg(not(feature = "alloc"))]
#[derive(Debug)]
pub struct Error {
    type_id: TypeId,
    code: u32,
}

impl Error {
    #[cfg(feature = "alloc")]
    pub fn downcast_ref<E: Errable + Sized>(&self) -> Option<&E> {
        self.inner.as_any().downcast_ref()
    }

    #[cfg(not(feature = "alloc"))]
    pub fn downcast<E: ErrableCode + Sized + 'static>(&self) -> Option<E> {
        if self.type_id == TypeId::of::<E>() {
            Some(E::from_error_code(self.code))
        } else {
            None
        }
    }
}

// This is a type T where 'From<T> for Error' is defined.
#[cfg(feature = "alloc")]
pub trait IntoError = std::error::Error; // Errable;
#[cfg(not(feature = "alloc"))]
pub trait IntoError = ErrableCode;

#[cfg(feature = "alloc")]
impl<T: Errable> From<T> for Error {
    fn from(v: T) -> Self {
        Self { inner: Box::new(v) }
    }
}

#[cfg(not(feature = "alloc"))]
impl<T: ErrableCode> From<T> for Error {
    fn from(v: T) -> Self {
        Error {
            type_id: Any::type_id(&v),
            code: v.error_code(),
        }
    }
}

pub trait Errable: Any + Display + Debug {
    fn as_any<'a>(&'a self) -> &'a dyn Any;
}

/// Trait implemented by error implementations which are representable as
/// trivial enums.
pub trait ErrableCode: 'static {
    fn from_error_code(code: u32) -> Self
    where
        Self: Sized;

    fn error_code(&self) -> u32;
}

pub type Result<T, E = Error> = core::result::Result<T, E>;

impl Errable for Infallible {
    fn as_any<'a>(&'a self) -> &'a dyn Any {
        self
    }
}

impl ErrableCode for Infallible {
    fn from_error_code(code: u32) -> Self {
        panic!()
    }

    fn error_code(&self) -> u32 {
        panic!()
    }
}

/*
derive(Errable) will attempt to implement both Errable and ErrableCode if possible.
- ErrableCode will be implemented if the enum has #[repr(u32)]
*/

// pub trait Errable: Display + Debug {}
