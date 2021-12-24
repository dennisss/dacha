#[cfg(feature = "std")]
mod error_failure;
#[cfg(feature = "std")]
pub use error_failure::*;

pub mod error_new;
#[cfg(not(feature = "std"))]
pub use error_new::*;

/*
pub mod errors {
    error_chain! {
        foreign_links {
            Io(::std::io::Error);
            ParseInt(::std::num::ParseIntError);
            CharTryFromError(::std::char::CharTryFromError);
            FromUtf8Error(::std::str::Utf8Error);
            FromUtf8ErrorString(::std::string::FromUtf8Error);
            FromUtf16Error(::std::string::FromUtf16Error);
            FromHexError(::hex::FromHexError);
            FromBase64Error(::base64::DecodeError);
            // Db(diesel::result::Error);
            // HTTP(hyper::Error);
        }

        errors {
            // A type of error returned while performing a request
            // It is generally appropriate to respond with this text as a 400 error
            // We will eventually standardize the codes such that higher layers can easily distinguish errors
            // API(code: u16, message: &'static str) {
            // 	display("API Error: {} '{}'", code, message)
            // }
            Parser(t: ParserErrorKind) {
                display("Parser: {:?}", t)
            }
        }
    }
}
*/
