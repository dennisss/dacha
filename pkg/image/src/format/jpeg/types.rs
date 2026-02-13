
use common::fixed::vec::FixedVec;
use common::bits::BitVector;

// TODO: We can store all codes with just 8 bits + a 8-bit length specifier since the
// rest of the bits are always ones.
//
// Currently this takes as a usize + usize + 2 bytes
//
// Intermediate optimization would be to get rid of one of the usizes.
pub type BitVector16 = BitVector<FixedVec<u8, 2>>;
