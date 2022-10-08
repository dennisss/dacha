mod edwards;
mod montgomery;
mod nist;
mod normal;

pub use self::edwards::*;
pub use self::montgomery::*;
pub use self::nist::*;
pub use self::normal::*;

// TODO: REALLY NEED tests with having more invalid signatures.

// TODO: Need precomputation of the montgomery modulus information for all
// curves.

// TODO: A lot of this code assumes that the prime used is divisible by the limb
// base used in SecureBigUint so that to_be_bytes doesn't add too many bytes.

// TODO: Don't forget to mask the last bit when decoding k or u?

// TODO: Now all of these are using modular arithmetic right now

// TODO: See https://en.wikipedia.org/wiki/Exponentiation_by_squaring#Montgomery's_ladder_technique as a method of doing power operations in constant time

// TODO: See start of https://tools.ietf.org/html/rfc7748#section-5 for the encode/decode functions

// pub fn decode_u_cord()

// TODO: Need a custom function for sqr (aka n^2)

// See also https://www.iacr.org/cryptodb/archive/2006/PKC/3351/3351.pdf
