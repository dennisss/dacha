// Huffman tree/coding

use std::cmp::Ordering;
use std::collections::HashMap;

use common::algorithms::merge_by;
use common::bits::*;
use common::errors::*;

// TODO: Remove debug from these
#[derive(Debug)]
enum HuffmanNode {
    Inner(Option<Box<HuffmanNode>>, Option<Box<HuffmanNode>>),
    Leaf(usize),
}

/// Binary tree container with values only at the leaf nodes.
/// Attempts to insert a code as nother code's prefix will fail.
///
/// NOTE: It is valid to insert a symbol multiple times with different codes.
/// NOTE: It also does not guarantee that every path leads to a root.
/// (so some reads may fail even if the underlying reader does not fail).
#[derive(Debug)]
pub struct HuffmanTree {
    // TODO: The max representable codelength in deflate is 15.

    // TODO: this does not need to be an option
    // TODO: The top node should not be allowed to be a leaf
    root: Option<HuffmanNode>,
}

#[derive(Clone, Debug)]
struct Coin {
    // Will be from 1 to 2^(L-1) where L is the maximum length.
    denomination: usize,

    symbol: usize,
}

#[derive(Clone, Debug)]
struct Package {
    // Total denomination of all contained coins.
    // denomination: usize,
    // Total
    value: usize,

    // TODO: We should be ableto implement this more efficiently without storing these.
    inner: Vec<Coin>,
}

/// A symbol and the length of the code used to encode it.
#[derive(Debug, PartialEq)]
pub struct SymbolLength {
    pub symbol: usize,
    pub length: usize,
}

pub type SparseSymbolLengths = Vec<SymbolLength>;

/// A symbol and the number of times it occurs in the data.
/// A vector of these would make up a sparse histogram.
#[derive(Debug, PartialEq)]
pub struct SymbolCount {
    pub symbol: usize,
    pub count: usize,
}

impl HuffmanTree {
    pub fn new() -> Self {
        HuffmanTree { root: None }
    }

    pub fn from_canonical_lens(lens: &[usize]) -> Result<Self> {
        let codes = huffman_canonical_codes_from_lens(lens)?;

        let mut tree = HuffmanTree::new();
        for (i, c) in codes.into_iter() {
            tree.insert(i, c)?;
        }

        Ok(tree)
    }

    pub fn insert(&mut self, symbol: usize, code: BitVector) -> Result<()> {
        assert!(code.len() > 0);

        let mut current_node = self.root.get_or_insert(HuffmanNode::Inner(None, None));

        for i in 0..code.len() {
            let b = code.get(i).unwrap();
            let next_node = match current_node {
                HuffmanNode::Leaf(_) => {
                    return Err(err_msg("A shorter prefix was already inserted"));
                }
                HuffmanNode::Inner(ref mut left, ref mut right) => {
                    if b == 0 {
                        left
                    } else {
                        right
                    }
                }
            };

            let val = Box::new(if i == code.len() - 1 {
                if let Some(_) = *next_node {
                    return Err(err_msg("A code already exists with this code as a prefix"));
                }

                HuffmanNode::Leaf(symbol)
            } else {
                HuffmanNode::Inner(None, None)
            });

            // NOTE: Will always insert.
            current_node = next_node.get_or_insert(val);
        }

        Ok(())
    }

    // NOTE: If not valid code is in the tree,
    pub fn read_code(&self, strm: &mut BitReader) -> Result<usize> {
        let (mut left, mut right) = match self.root {
            Some(HuffmanNode::Inner(ref l, ref r)) => (l, r),
            // NOTE: This will also occur if the root is a leaf node (which should never be
            // constructable)
            _ => {
                return Err(err_msg("Empty tree"));
            }
        };

        loop {
            let b = strm.read_bits_exact(1)?;
            let next_node = if b == 0 { left } else { right };

            match next_node {
                Some(box HuffmanNode::Leaf(sym)) => {
                    return Ok(*sym);
                }
                Some(box HuffmanNode::Inner(l, r)) => {
                    left = l;
                    right = r;
                }
                None => {
                    return Err(err_msg("Invalid code prefix read"));
                }
            }
        }
    }

    // Builds a huffman encoding for some data under the constraint that code
    // lengths are <= a requested maximum length.
    //
    // It is still more useful to have the raw lens
    // TODO: Rename as we aren't actually building a tree?
    //
    // Output is pairs of Symbol, Codelength
    pub fn build_length_limited_tree(
        symbols: &[usize],
        max_code_length: usize,
    ) -> Result<Vec<SymbolLength>> {
        // Get frequencies of symbols.
        // TODO: Should support raw input of a histogram.
        let mut freqs_map = std::collections::HashMap::new();
        for s in symbols {
            let mut f = freqs_map.get(s).cloned().unwrap_or(0);
            f += 1;
            freqs_map.insert(*s, f);
        }

        // Sorted histogram of symbol frequencies.
        let mut freqs = freqs_map
            .into_iter()
            .map(|(s, c)| SymbolCount {
                symbol: s,
                count: c,
            })
            .collect::<Vec<_>>();

        freqs.sort_unstable_by(|a, b| {
            if a.count == b.count {
                // If frequencies are equal, sort by symbol.
                a.symbol.partial_cmp(&b.symbol).unwrap()
            } else {
                // Otherwise sort by frequencies.
                a.count.partial_cmp(&b.count).unwrap()
            }
        });

        // Creates the list of coins for the denomination 2^(-1).
        //
        // NOTE: This is the list pre-pairing/merging which contains all initial
        // coins of this single denomination.
        // TODO: Instead return an iterator?
        let build_denom_list = |i| {
            // This list will have denomination 2^(-i)
            let mut denom = vec![];
            denom.reserve(freqs.len());

            // Append initial coins of this denomination to the list in order of increasing
            // value.
            for f in freqs.iter() {
                let coin = Coin {
                    denomination: (1 << i) as usize, // (-(i as f32)).exp2(),
                    // value: f.count,
                    symbol: f.symbol,
                };

                denom.push(Package {
                    // denomination: coin.denomination,
                    value: f.count,
                    inner: vec![coin],
                });
            }

            denom
        };

        // TODO: Implement with into_iter.
        let package = |list: Vec<Package>| {
            let mut out = vec![];
            let npairs = list.len() / 2;
            out.reserve(npairs);
            for i in 0..npairs {
                let mut p = list[2 * i].clone();

                let p2 = &list[2 * i + 1];
                p.value += p2.value;
                p.inner.extend_from_slice(&p2.inner);

                out.push(p);
            }

            out
        };

        let compare_pkg = |a: &Package, b: &Package| {
            match a.value.partial_cmp(&b.value).unwrap() {
                // No two packages are ever the same so merge them arbitrarily.
                Ordering::Equal => Ordering::Less,
                o @ _ => o,
            }
        };

        // Final packaged/merged flat list of coins.
        let mut list: Vec<Package> = vec![];

        for i in 0..max_code_length {
            let denom = build_denom_list(i);
            list = merge_by(list, denom, compare_pkg);
            list = package(list);
        }

        // Map of how many times coins occur with each symbol.
        let mut count_map = std::collections::HashMap::new();

        let mut total = 0;
        let desired_total = (freqs.len() - 1) * (1 << max_code_length);
        let mut package_i = 0;
        let mut coin_i = 0;
        while total < desired_total {
            if package_i >= list.len() {
                break;
            }

            let p = &list[package_i];

            if coin_i >= p.inner.len() {
                package_i += 1;
                coin_i = 0;
                continue;
            }

            let c = &p.inner[coin_i];
            coin_i += 1;

            total += c.denomination;

            // Increment count for this symbol.
            let mut f = count_map.get(&c.symbol).cloned().unwrap_or(0);
            f += 1;
            count_map.insert(c.symbol, f);
        }

        if total != desired_total {
            return Err(err_msg("Failed"));
        }

        // The length of each symbol will be the number of times it occured in the
        // lowest packages.
        let mut lens = count_map
            .into_iter()
            .map(|(symbol, count)| SymbolLength {
                symbol,
                length: count,
            })
            .collect::<Vec<_>>();

        // Ensure that we were able to produce a length for every original symbol.
        if lens.len() != freqs.len() {
            return Err(err_msg("Failed 2"));
        }

        // Sanity check that we did not generate any code lengths longer than the
        // requested limit.
        for l in lens.iter() {
            if l.length > max_code_length {
                return Err(err_msg("Some lengths ended up too large"));
            }
        }

        // Sort symbol by symbol id.
        lens.sort_unstable_by(|a, b| a.symbol.partial_cmp(&b.symbol).unwrap());

        Ok(lens)
    }
}

/// Writes
pub struct HuffmanEncoder {
    codes: HashMap<usize, BitVector>,
}

impl HuffmanEncoder {
    pub fn from_canonical_lens(lens: &[usize]) -> Result<HuffmanEncoder> {
        let codelist = huffman_canonical_codes_from_lens(lens)?;

        let mut codes = HashMap::new();
        for (symbol, code) in codelist.into_iter() {
            codes.insert(symbol, code);
        }

        Ok(HuffmanEncoder { codes })
    }

    pub fn write_symbol(&self, symbol: usize, writer: &mut dyn BitWrite) -> Result<()> {
        let code = match self.codes.get(&symbol) {
            Some(c) => c,
            None => {
                return Err(err_msg("Unkown symbol given to encoder"));
            }
        };

        writer.write_bitvec(code)
    }
}

// TODO: We must enforce maximum code lengths of 64bits everywhere

fn code2vec(code: usize, len: usize) -> BitVector {
    let mut v = BitVector::new();
    for i in (0..len).rev() {
        let b = ((code >> i) & 0b1) as u8;
        v.push(b);
    }

    assert_eq!(code >> len, 0);

    v
}

// TODO: Doesn't seem to work when there are 0 length symbols
// TODO: For deflate, it is more efficient to do this from the sparse form
// See RFC1951 3.2.1
pub fn huffman_canonical_codes_from_lens(lens: &[usize]) -> Result<Vec<(usize, BitVector)>> {
    let max_len = lens.iter().fold(0, |x, y| std::cmp::max(x, *y));

    // Count of how many times each length occurs in all codes.
    let mut bl_count = vec![];
    bl_count.resize(max_len + 1, 0);
    for l in lens {
        bl_count[*l] += 1;
    }

    // Zero length codes should not contribute to constructing the codes.
    bl_count[0] = 0;

    let mut next_code: Vec<usize> = vec![];
    next_code.resize(max_len + 1, 0);

    let mut code: usize = 0;
    for bits in 1..(max_len + 1) {
        code = (code + bl_count[bits - 1]) << 1;
        next_code[bits] = code;
    }

    // TODO: Validate that the codes are representable in the # of bits

    // TODO: We should be able to reserve this vector memory.
    let mut out = vec![];
    out.reserve_exact(lens.len()); // NOTE: This may over-allocate.

    // TODO: This should actually go up to max_code
    for (i, len) in lens.iter().enumerate() {
        if *len != 0 {
            let code = next_code[*len];
            next_code[*len] += 1;

            out.push((i, code2vec(code, *len)));
        }

        // TODO: Handle the 0 length case nicely
    }

    Ok(out)
}

/// Given a list of sorted sparse lengths for each symbol, generates a dense
/// list such that the first length is for symbol 0 and the last length is for
/// the largest symbol present in the input list.
///
/// NOTE: This assumes that the symbols are sorted and distint
pub fn dense_symbol_lengths(lens: &Vec<SymbolLength>) -> Vec<usize> {
    let mut out = vec![];
    if let Some(v) = lens.last() {
        out.reserve(v.length);
    } else {
        return out;
    }

    for v in lens.iter() {
        assert!(out.len() <= v.symbol);
        out.resize(v.symbol, 0);
        out.push(v.length);
    }

    out
}

pub struct HashHuffmanTree {
    data: HashMap<u32, u8>,
}

impl HashHuffmanTree {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
    pub fn insert(&mut self, value: u8, code: BitVector) -> Result<()> {
        let mut code_num = 1;
        for i in 0..code.len() {
            code_num = (code_num << 1) | (code.get(i).unwrap() as u32);
        }

        if self.data.contains_key(&code_num) {
            return Err(err_msg("Duplicate code"));
        }

        self.data.insert(code_num, value);

        Ok(())
    }

    pub fn read_code(&self, reader: &mut BitReader) -> Result<u8> {
        let mut code_num = 1;
        for i in 0..31 {
            let bit = reader.read_bits_exact(1)?;
            code_num = (code_num << 1) | (bit as u32);
            if let Some(value) = self.data.get(&code_num) {
                return Ok(*value);
            }
        }

        Err(err_msg("Unknown code prefix"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn code2vec_works() {
        assert_eq!(&format!("{:?}", code2vec(0, 5)), "'00000'");
        assert_eq!(&format!("{:?}", code2vec(5, 4)), "'0101'");
    }

    #[test]
    fn code2vec_test() {
        assert_eq!(code2vec(0b101100, 7), "0101100".try_into().unwrap());
    }

    // TODO: Add test cases for huffman tree failure cases.
    #[test]
    fn huffman_tree_test() {
        let mut tree = HuffmanTree::new();
        tree.insert(5, "001".try_into().unwrap()).unwrap();
        tree.insert(2, "01".try_into().unwrap()).unwrap();
        tree.insert(3, "0000".try_into().unwrap()).unwrap();

        let data = vec![0b01000000, 0b00000001];
        let mut c = std::io::Cursor::new(data);
        let mut strm = BitReader::new(&mut c);

        assert_eq!(3, tree.read_code(&mut strm).unwrap());
        assert_eq!(5, tree.read_code(&mut strm).unwrap());
        assert_eq!(2, tree.read_code(&mut strm).unwrap());
    }

    #[test]
    fn huffman_codes_from_lens_test() {
        let codes = huffman_canonical_codes_from_lens(&[3, 3, 3, 3, 3, 2, 4, 4]).unwrap();

        assert_eq!(
            &codes,
            &[
                (0, "010".try_into().unwrap()),
                (1, "011".try_into().unwrap()),
                (2, "100".try_into().unwrap()),
                (3, "101".try_into().unwrap()),
                (4, "110".try_into().unwrap()),
                (5, "00".try_into().unwrap()),
                (6, "1110".try_into().unwrap()),
                (7, "1111".try_into().unwrap())
            ]
        );
    }

    #[test]
    fn huffman_codes_from_lens_test_zeros() {
        let codes = huffman_canonical_codes_from_lens(&[0, 0, 3, 3, 3, 3, 3, 2, 4, 4]).unwrap();

        assert_eq!(
            &codes,
            &[
                (2, "010".try_into().unwrap()),
                (3, "011".try_into().unwrap()),
                (4, "100".try_into().unwrap()),
                (5, "101".try_into().unwrap()),
                (6, "110".try_into().unwrap()),
                (7, "00".try_into().unwrap()),
                (8, "1110".try_into().unwrap()),
                (9, "1111".try_into().unwrap())
            ]
        );
    }

    #[test]
    fn build_length_limited_tree_test() {
        let data = vec![1, 2, 2, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6];
        let out = HuffmanTree::build_length_limited_tree(&data, 3).unwrap();

        let expected = vec![
            SymbolLength {
                symbol: 1,
                length: 3,
            },
            SymbolLength {
                symbol: 2,
                length: 3,
            },
            SymbolLength {
                symbol: 3,
                length: 3,
            },
            SymbolLength {
                symbol: 4,
                length: 3,
            },
            SymbolLength {
                symbol: 5,
                length: 2,
            },
            SymbolLength {
                symbol: 6,
                length: 2,
            },
        ];

        assert_eq!(out, expected);
    }
}
