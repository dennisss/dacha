pub fn xor(a: &[u8], b: &[u8], out: &mut [u8]) {
    // TODO: Any block > 16 in length can be sped up using this.
    // if a.len() == 16 {
    // 	let ai = to_m128i(a);
    // 	let bi = to_m128i(b);

    // }

    for i in 0..a.len() {
        out[i] = a[i] ^ b[i];
    }
}

pub fn xor_inplace(a: &[u8], b: &mut [u8]) {
    for i in 0..b.len() {
        b[i] ^= a[i];
    }
}
