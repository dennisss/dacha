/// Based on the Google fingerprinting algorithm originally releases here:
/// https://searchcode.com/codesearch/view/63817931/
/// ^ Documentation also borrowed from that souce.
/// 
/// Copyright Google, Apache 2.0 license
/// 
/// This is a non-cryptographically secure hashing function meant for collision
/// avoidance and speed more than anything else.

fn FingerprintCat2011(fp1: u64, fp2: u64) -> u64 {
	// Two big prime numbers.
	const kMul1: u64 = 0xc6a4a7935bd1e995;
	const kMul2: u64 = 0x228876a7198b743;
	let a = fp1.wrapping_mul(kMul1)
		.wrapping_add(fp2.wrapping_mul(kMul2));
	// Note: The following line also makes sure we never return 0 or 1, because
	// we will only add something to 'a' if there are any MSBs (the remaining
	// bits after the shift) being 0, in which case wrapping around would not
	// happen.
	return a.wrapping_add(!a >> 47);
}

// This should be better (collision-wise) than the default hash<std::string>,
// without being much slower. It never returns 0 or 1.
pub fn Fingerprint2011(mut bytes: &[u8]) -> u64 {
  // Some big prime number.
  let mut fp: u64 = 0xa5b85c5e198ed849;

  while (bytes + 8 <= end) {
    fp = FingerprintCat2011(fp, *(reinterpret_cast<const uint64*>(bytes)));
    bytes += 8;
  }
  // Note: we don't care about "consistency" (little or big endian) between
  // the bulk and the suffix of the message.
  uint64 last_bytes = 0;
  while (bytes < end) {
    last_bytes += *bytes;
    last_bytes <<= 8;
    bytes++;
  }
  return FingerprintCat2011(fp, last_bytes);
}