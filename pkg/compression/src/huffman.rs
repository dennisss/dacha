// Huffman tree/coding

use std::cmp::Ordering;
use common::errors::*;
use common::algorithms::merge_by;
use super::bits::*;

enum HuffmanNode {
	Inner(Option<Box<HuffmanNode>>, Option<Box<HuffmanNode>>),
	Leaf(usize)
}

/// Binary tree container with values only at the leaf nodes.
/// Attempts to insert a code as nother code's prefix will fail.
/// 
/// NOTE: It is valid to insert a symbol multiple times with different codes.
/// NOTE: It also does not guarantee that every path leads to a root.
/// (so some reads may fail even if the underlying reader does not fail).
pub struct HuffmanTree {
	// TODO: The max representable codelength in deflate is 15.

	// TODO: this does not need to be an option
	// TODO: The top node should not be allowed to be a leaf
	root: Option<HuffmanNode>
}

#[derive(Clone, Debug)]
struct Coin {
	// Will be from 1 to 2^(L-1) where L is the maximum length.
	denomination: usize,

	symbol: usize
}

#[derive(Clone, Debug)]
struct Package {
	// Total denomination of all contained coins.
	// denomination: usize,
	// Total 
	value: usize,

	// TODO: We should be ableto implement this more efficiently without storing these.
	inner: Vec<Coin>
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

		let mut current_node = self.root.get_or_insert(
			HuffmanNode::Inner(None, None));
		
		for i in 0..code.len() {
			let b = code.get(i);
			let next_node = match current_node {
				HuffmanNode::Leaf(_) => {
					return Err("A shorter prefix was already inserted".into());
				},
				HuffmanNode::Inner(ref mut left, ref mut right) => {
					if b == 0 {
						left
					} else {
						right 
					}
				}
			};

			let val = Box::new(
				if i == code.len() - 1 {
					if let Some(_) = *next_node {
						return Err("A code already exists with this code as a prefix".into());
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
	pub fn read_code(&self, strm: &mut BitStream) -> Result<usize> {
		let (mut left, mut right) = match self.root {
			Some(HuffmanNode::Inner(ref l, ref r)) => (l, r),
			// NOTE: This will also occur if the root is a leaf node (which should never be constructable)
			_ => { return Err("Empty tree".into()); } 
		};

		loop {
			let b = strm.read_bits(1)?.ok_or(Error::from("Unexpected end of input"))?;
			let next_node = 
				if b == 0 {
					left
				} else {
					right
				};

			match next_node {
				Some(box HuffmanNode::Leaf(sym)) => { return Ok(*sym); },
				Some(box HuffmanNode::Inner(l, r)) => {
					left = l; right = r;
				},
				None => { return Err("Invalid code prefix read".into()); }
			}
		}
	}

	// It is still more useful to have the raw lens
	// TODO: Rename as we aren't actually building a tree?
	pub fn build_length_limited_tree(symbols: &[usize], max_code_length: usize) -> Result<Vec<(usize, usize)>> {
		// Get frequencies of symbols.
		let mut freqs_map = std::collections::HashMap::new();
		for s in symbols {
			let mut f = freqs_map.get(s).cloned().unwrap_or(0);
			f += 1;
			freqs_map.insert(*s, f);
		}

		// List of symbols of the form (symbol, count).
		let mut freqs = freqs_map.into_iter().collect::<Vec<(usize, usize)>>();
		freqs.sort_unstable_by(|(sa, ca), (sb, cb)| {
			if ca == cb {
				// If frequencies are equal, sort by symbol.
				sa.partial_cmp(sb).unwrap()
			} else {
				// Otherwise sort by frequencies.
				ca.partial_cmp(cb).unwrap()
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

			// Append initial coins of this denomination to the list in order of increasing value. 
			for f in freqs.iter().cloned() {
				let coin = Coin {
					denomination: (1 << i) as usize, // (-(i as f32)).exp2(),
					// value: f.1,
					symbol: f.0
				};

				denom.push(Package {
					// denomination: coin.denomination,
					value: f.1,
					inner: vec![coin]
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
				let mut p = list[2*i].clone();

				let p2 = &list[2*i + 1];
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
				o @ _ => o
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
		let desired_total = (freqs.len() - 1)*(1 << max_code_length); 
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
			return Err("Failed".into());
		}

		let mut lens = count_map.into_iter().collect::<Vec<(usize, usize)>>();

		// Ensure that we were able to produce a length for every original symbol.
		if lens.len() != freqs.len() {
			return Err("Failed 2".into());
		}

		// Sort symbol by symbol id.
		lens.sort_unstable_by(|(sa, _), (sb, _)| {
			sa.partial_cmp(sb).unwrap()
		});

		Ok(lens)
	}
}

/// Writes 
pub struct HuffmanEncoder {
	codes: std::collections::HashMap<usize, BitVector>
}



// TODO: We must enforce maximum code lengths of 64bits everywhere

fn code2vec(code: usize, len: usize) -> BitVector {
	let mut v = BitVector::new();
	for i in (0..len).rev() {
		let b = ((code >> i) & 0b1) as u8;
		v.push(b);
	}

	v
}


// See RFC1951 3.2.1
pub fn huffman_canonical_codes_from_lens(lens: &[usize])
	-> Result<Vec<(usize, BitVector)>> {
	let mut max_len = lens.iter().fold(0, |x, y| std::cmp::max(x, *y)); 

	let mut bl_count = vec![];
	bl_count.resize(max_len + 1, 0);
	for l in lens {
		bl_count[*l] += 1;
	}

	let mut next_code = vec![];
	next_code.resize(max_len + 1, 0);
	
	let mut code = 0;
	for bits in 1..(max_len + 1) {
		code = (code + bl_count[bits - 1]) << 1;
		next_code[bits] = code;
	}

	// TODO: Validate that the codes are representable in the # of bits

	// TODO: We should be able to reserve this vector memory.
	let mut out = vec![];

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


#[cfg(test)]
mod tests {
	use super::*;

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
		let mut strm = BitStream::new(&mut c);

		assert_eq!(3, tree.read_code(&mut strm).unwrap());
		assert_eq!(5, tree.read_code(&mut strm).unwrap());
		assert_eq!(2, tree.read_code(&mut strm).unwrap());
	}

	#[test]
	fn huffman_codes_from_lens_test() {
		let codes = huffman_canonical_codes_from_lens(&[
			3, 3, 3, 3, 3, 2, 4, 4
		]).unwrap();

		assert_eq!(&codes, &[
			(0, "010".try_into().unwrap()),
			(1, "011".try_into().unwrap()),
			(2, "100".try_into().unwrap()),
			(3, "101".try_into().unwrap()),
			(4, "110".try_into().unwrap()),
			(5, "00".try_into().unwrap()),
			(6, "1110".try_into().unwrap()),
			(7, "1111".try_into().unwrap())
		]);
	}
}
