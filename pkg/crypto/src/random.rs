use alloc::boxed::Box;
use std::f64::consts::PI;
use std::num::Wrapping;
use std::sync::Arc;
use std::vec::Vec;

use common::bytes::{Buf, Bytes};
use common::io::Readable;
use common::{ceil_div, errors::*};
use executor::sync::Mutex;
use file::LocalFile;
use math::big::{BigUint, SecureBigUint};
use math::integer::Integer;

use crate::chacha20::*;

const MAX_BYTES_BEFORE_RESEED: usize = 1024 * 1024 * 1024; // 1GB

lazy_static! {
    static ref GLOBAL_RNG_STATE: GlobalRng = GlobalRng::new();
}

/// Gets a lazily initialized reference to a globally shared random number
/// generator.
///
/// This is seeded on the first random generation.
///
/// The implementation can be assumed to be secure for cryptographic purposes
/// but may not be very fast.
///
/// TODO: We should disallow re-seeding this RNG.
pub fn global_rng() -> GlobalRng {
    GLOBAL_RNG_STATE.clone()
}

/// Call me if you want a cheap but insecure RNG seeded by the current system
/// time.
pub fn clocked_rng() -> MersenneTwisterRng {
    let mut rng = MersenneTwisterRng::mt19937();
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    rng.seed_u32(seed);
    rng
}

/// Generates secure random bytes suitable for cryptographic key generation.
/// This will wait for sufficient entropy to accumulate in the system.
///
/// Once done, the provided buffer will be filled with the random bytes to the
/// end.
pub async fn secure_random_bytes(buf: &mut [u8]) -> Result<()> {
    // See http://man7.org/linux/man-pages/man7/random.7.html
    // TODO: Reuse the file handle across calls.
    let mut f = LocalFile::open("/dev/random")?;
    f.read_exact(buf).await?;
    Ok(())
}

/// Securely generates a random value in the range '[lower, upper)'.
///
/// This is implemented to give every integer in the range the same probabiity
/// of being output.
///
/// NOTE: Both the 'lower' and 'upper' numbers should be publicly known for this
/// to be secure.
///
/// The output integer will have the same width as the 'upper' integer.
pub async fn secure_random_range(
    lower: &SecureBigUint,
    upper: &SecureBigUint,
) -> Result<SecureBigUint> {
    if upper.byte_width() == 0 || upper <= lower {
        return Err(err_msg("Invalid upper/lower range"));
    }

    let mut buf = vec![];
    buf.resize(upper.byte_width(), 0);

    let mut num_bytes = ceil_div(upper.value_bits(), 8);

    let msb_mask: u8 = {
        let r = upper.value_bits() % 8;
        if r == 0 {
            0xff
        } else {
            !((1 << (8 - r)) - 1)
        }
    };

    // TODO: Refactor out retrying. Instead shift to 0
    loop {
        secure_random_bytes(&mut buf[0..num_bytes]).await?;

        buf[num_bytes - 1] &= msb_mask;

        let n = SecureBigUint::from_le_bytes(&buf);

        // TODO: This *must* be a secure comparison (which it isn't right now).
        if &n >= lower && &n < upper {
            return Ok(n);
        }
    }
}

pub trait Rng {
    fn seed_size(&self) -> usize;

    fn seed(&mut self, new_seed: &[u8]);

    fn generate_bytes(&mut self, output: &mut [u8]);
}

#[async_trait]
pub trait SharedRng: 'static + Send + Sync {
    /// Number of bytes used to seed this RNG.
    fn seed_size(&self) -> usize;

    /// Should reset the state of the RNG based on the provided seed.
    /// Calling generate_bytes after calling reseed with the same seed should
    /// always produce the same result.
    async fn seed(&self, new_seed: &[u8]);

    async fn generate_bytes(&self, output: &mut [u8]);
}

#[derive(Clone)]
pub struct GlobalRng {
    state: Arc<GlobalRngState>,
}

struct GlobalRngState {
    bytes_since_reseed: Mutex<usize>,
    rng: ChaCha20RNG,
}

impl GlobalRng {
    fn new() -> Self {
        Self {
            state: Arc::new(GlobalRngState {
                bytes_since_reseed: Mutex::new(std::usize::MAX),
                rng: ChaCha20RNG::new(),
            }),
        }
    }
}

#[async_trait]
impl SharedRng for GlobalRng {
    fn seed_size(&self) -> usize {
        0
    }

    async fn seed(&self, _new_seed: &[u8]) {
        // Global RNG can't be manually reseeding
        panic!();
    }

    async fn generate_bytes(&self, output: &mut [u8]) {
        {
            let mut counter = self.state.bytes_since_reseed.lock().await;
            if *counter > MAX_BYTES_BEFORE_RESEED {
                let mut new_seed = vec![0u8; self.state.rng.seed_size()];
                secure_random_bytes(&mut new_seed).await.unwrap();
                self.state.rng.seed(&new_seed).await;
                *counter = 0;
            }

            // NOTE: For now we ignore the case of a user requesting a quantity that
            // partially exceeds our max threshold.
            *counter += output.len();
        }

        self.state.rng.generate_bytes(output).await
    }
}

/// Sample random number generator based on ChaCha20
///
/// - During initialization and periodically afterwards, we (re-)generate the
///   256-bit key from a 'true random' source (/dev/random).
/// - When a key is selected, we reset the nonce to 0.
/// - The nonce is incremented by 1 for each block we encrypt.
/// - The plaintext to be encrypted is the system time at key creation in
///   nanoseconds.
/// - All of the above are re-seeding in the background every 30 seconds.
/// - Random bytes are generated by encrypting the plaintext with the current
///   nonce and key.
pub struct ChaCha20RNG {
    state: Mutex<ChaCha20RNGState>,
}

#[derive(Clone)]
struct ChaCha20RNGState {
    key: [u8; CHACHA20_KEY_SIZE],
    nonce: u64,
    plaintext: [u8; CHACHA20_BLOCK_SIZE],
}

impl ChaCha20RNG {
    /// Creates a new instance of the rng with a fixed 'zero' seed.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ChaCha20RNGState {
                key: [0u8; CHACHA20_KEY_SIZE],
                nonce: 0,
                plaintext: [0u8; CHACHA20_BLOCK_SIZE],
            }),
        }
    }
}

#[async_trait]
impl SharedRng for ChaCha20RNG {
    fn seed_size(&self) -> usize {
        CHACHA20_KEY_SIZE + CHACHA20_BLOCK_SIZE
    }

    async fn seed(&self, new_seed: &[u8]) {
        let mut state = self.state.lock().await;
        state.nonce = 0;
        state.key.copy_from_slice(&new_seed[0..CHACHA20_KEY_SIZE]);
        state
            .plaintext
            .copy_from_slice(&new_seed[CHACHA20_KEY_SIZE..]);
    }

    async fn generate_bytes(&self, mut output: &mut [u8]) {
        let state = {
            let mut guard = self.state.lock().await;
            let cur_state = guard.clone();
            guard.nonce += 1;
            cur_state
        };

        let mut nonce = [0u8; CHACHA20_NONCE_SIZE];
        nonce[0..8].copy_from_slice(&state.nonce.to_ne_bytes());

        let mut chacha = ChaCha20::new(&state.key, &nonce);

        while !output.is_empty() {
            let mut output_block = [0u8; CHACHA20_BLOCK_SIZE];
            chacha.encrypt(&state.plaintext, &mut output_block);

            let n = std::cmp::min(output_block.len(), output.len());
            output[0..n].copy_from_slice(&output_block[0..n]);
            output = &mut output[n..];
        }
    }
}

#[async_trait]
pub trait SharedRngExt {
    async fn shuffle<T: Send + Sync>(&self, elements: &mut [T]);

    async fn uniform<T: RngNumber>(&self) -> T;

    async fn between<T: RngNumber>(&self, min: T, max: T) -> T;
}

#[async_trait]
impl<R: SharedRng> SharedRngExt for R {
    async fn shuffle<T: Send + Sync>(&self, elements: &mut [T]) {
        for i in 0..elements.len() {
            let j = self.uniform::<usize>().await % elements.len();
            elements.swap(i, j);
        }
    }

    async fn uniform<T: RngNumber>(&self) -> T {
        let mut buf = T::Buffer::default();
        self.generate_bytes(buf.as_mut()).await;
        T::uniform_buffer(buf)
    }

    async fn between<T: RngNumber>(&self, min: T, max: T) -> T {
        let mut buf = T::Buffer::default();
        self.generate_bytes(buf.as_mut()).await;
        T::between_buffer(buf, min, max)
    }
}

pub trait RngNumber: Send + Sync + 'static {
    type Buffer: Send + Sync + Sized + Default + AsMut<[u8]>;

    fn uniform_buffer(random: Self::Buffer) -> Self;

    fn between_buffer(random: Self::Buffer, min: Self, max: Self) -> Self;
}

macro_rules! ensure_positive {
    ($value:ident, I) => {
        if $value < 0 {
            $value * -1
        } else {
            $value
        }
    };
    ($value:ident, U) => {
        $value
    };
}

macro_rules! impl_rng_number_integer {
    ($num:ident, $type_prefix:ident) => {
        impl RngNumber for $num {
            type Buffer = [u8; std::mem::size_of::<$num>()];

            fn uniform_buffer(random: Self::Buffer) -> Self {
                Self::from_le_bytes(random)
            }

            fn between_buffer(random: Self::Buffer, min: Self, max: Self) -> Self {
                assert!(max >= min);

                let mut num = Self::uniform_buffer(random);

                // In rust (negative_number % positive_number) = negative_number
                num = ensure_positive!(num, $type_prefix);

                // Convert to [0, range)
                let range = max - min;
                num = num % range;

                // Convert to [min, max)
                num += min;

                num
            }
        }
    };
}

impl_rng_number_integer!(u8, U);
impl_rng_number_integer!(i8, I);
impl_rng_number_integer!(u16, U);
impl_rng_number_integer!(i16, I);
impl_rng_number_integer!(u32, U);
impl_rng_number_integer!(i32, I);
impl_rng_number_integer!(u64, U);
impl_rng_number_integer!(i64, I);
impl_rng_number_integer!(usize, U);
impl_rng_number_integer!(isize, I);

macro_rules! impl_rng_number_float {
    ($float_type:ident, $int_type:ident, $fraction_bits:expr, $zero_exponent:expr) => {
        impl RngNumber for $float_type {
            type Buffer = [u8; std::mem::size_of::<$float_type>()];

            fn uniform_buffer(random: Self::Buffer) -> Self {
                Self::from_le_bytes(random)
            }

            fn between_buffer(mut random: Self::Buffer, min: Self, max: Self) -> Self {
                assert!(max >= min);

                let mut num = $int_type::from_le_bytes(random);

                // Clear the sign and exponent bits.
                num &= (1 << $fraction_bits) - 1;
                // Set the exponent to '0'. So the number will be (1 + fraction) * 2^0
                num |= $zero_exponent << $fraction_bits;

                random = num.to_le_bytes();

                // This will in the range [0, 1).
                let f = Self::from_le_bytes(random) - 1.0;

                // Convert to [min, max).
                let range = max - min;
                f * range + min
            }
        }
    };
}

impl_rng_number_float!(f32, u32, 23, 127);
impl_rng_number_float!(f64, u64, 52, 1023);

pub trait RngExt {
    fn shuffle<T>(&mut self, elements: &mut [T]);

    fn uniform<T: RngNumber>(&mut self) -> T;

    fn between<T: RngNumber>(&mut self, min: T, max: T) -> T;

    fn choose<'a, T>(&mut self, elements: &'a [T]) -> &'a T;
}

impl<R: Rng + ?Sized> RngExt for R {
    fn shuffle<T>(&mut self, elements: &mut [T]) {
        for i in 0..elements.len() {
            let j = self.uniform::<usize>() % elements.len();
            elements.swap(i, j);
        }
    }

    /// Returns a completely random number anywhere in the range of the number
    /// type. Every number is equally probably of occuring.
    fn uniform<T: RngNumber>(&mut self) -> T {
        let mut buf = T::Buffer::default();
        self.generate_bytes(buf.as_mut());
        T::uniform_buffer(buf)
    }

    /// Returns a uniform random number in the range [min, max).
    ///
    /// Limitations:
    /// - 'max' must be >= 'min'.
    /// - For signed integer types for N bits, 'max' - 'min' must fit in N-1
    ///   bits.
    fn between<T: RngNumber>(&mut self, min: T, max: T) -> T {
        let mut buf = T::Buffer::default();
        self.generate_bytes(buf.as_mut());
        T::between_buffer(buf, min, max)
    }

    fn choose<'a, T>(&mut self, elements: &'a [T]) -> &'a T {
        assert!(!elements.is_empty(), "Choosing from empty list");

        let n = self.uniform::<usize>();
        &elements[n % elements.len()]
    }
}

pub const MT_DEFAULT_SEED: u32 = 5489;

pub struct MersenneTwisterRng {
    w: u32,
    n: usize,
    m: usize,
    r: u32,
    a: u32, //
    b: u32,
    c: u32,
    s: u32,
    t: u32,
    u: u32,
    d: u32,
    l: u32,
    f: u32,

    x: Vec<u32>,
    index: usize,
}

impl MersenneTwisterRng {
    // TODO: Add a simple time seeded implementation.

    pub fn mt19937() -> Self {
        Self {
            w: 32,
            n: 624,
            m: 397,
            r: 31,
            a: 0x9908B0DF,
            u: 11,
            d: 0xffffffff,
            s: 7,
            b: 0x9D2C5680,
            t: 15,
            c: 0xEFC60000,
            l: 18,

            f: 1812433253,

            x: vec![],
            index: 0,
        }
    }

    pub fn seed_u32(&mut self, seed: u32) {
        self.x.resize(self.n, 0);

        self.index = self.n;
        self.x[0] = seed;
        for i in 1..self.n {
            self.x[i] = (self.x[i - 1] ^ (self.x[i - 1] >> (self.w - 2)))
                .wrapping_mul(self.f)
                .wrapping_add(i as u32);
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        if self.x.is_empty() {
            self.seed_u32(MT_DEFAULT_SEED);
        }

        if self.index >= self.n {
            self.twist();
        }

        let mut y = self.x[self.index];
        y ^= (y >> self.u) & self.d;
        y ^= (y << self.s) & self.b;
        y ^= (y << self.t) & self.c;
        y ^= y >> self.l;

        self.index += 1;
        y
    }

    fn twist(&mut self) {
        let w_mask = 1u32.checked_shl(self.w).unwrap_or(0).wrapping_sub(1);

        let upper_mask = (w_mask << self.r) & w_mask;
        let lower_mask = (!upper_mask) & w_mask;

        self.index = 0;

        for i in 0..self.n {
            let x = (self.x[i] & upper_mask) | (self.x[(i + 1) % self.x.len()] & lower_mask);
            let mut x_a = x >> 1;
            if x & 1 != 0 {
                x_a = x_a ^ self.a;
            }

            self.x[i] = self.x[(i + self.m) % self.x.len()] ^ x_a;
        }
    }
}

impl Rng for MersenneTwisterRng {
    fn seed_size(&self) -> usize {
        std::mem::size_of::<u32>()
    }

    fn seed(&mut self, new_seed: &[u8]) {
        assert_eq!(new_seed.len(), std::mem::size_of::<u32>());
        let seed_num = u32::from_le_bytes(*array_ref![new_seed, 0, 4]);
        self.seed_u32(seed_num);
    }

    fn generate_bytes(&mut self, output: &mut [u8]) {
        // NOTE: All of the 4's in here are std::mem::size_of::<u32>()
        let n = output.len() / 4;
        let r = output.len() % 4;

        for i in 0..n {
            *array_mut_ref![output, 4 * i, 4] = self.next_u32().to_le_bytes();
        }

        if r != 0 {
            let v = self.next_u32().to_le_bytes();
            let i = output.len() - r;
            output[i..].copy_from_slice(&v[0..r]);
        }
    }
}

pub struct FixedBytesRng {
    data: Bytes,
}

impl FixedBytesRng {
    pub fn new<T: Into<Bytes>>(data: T) -> Self {
        Self { data: data.into() }
    }
}

impl Rng for FixedBytesRng {
    fn seed_size(&self) -> usize {
        panic!();
    }

    fn seed(&mut self, _new_seed: &[u8]) {
        panic!();
    }

    fn generate_bytes(&mut self, output: &mut [u8]) {
        if output.len() > self.data.len() {
            panic!();
        }

        output.copy_from_slice(&self.data[0..output.len()]);
        self.data.advance(output.len());
    }
}

pub struct NormalDistribution {
    mean: f64,
    stddev: f64,
    next_number: Option<f64>,
}

impl NormalDistribution {
    pub fn new(mean: f64, stddev: f64) -> Self {
        Self {
            mean,
            stddev,
            next_number: None,
        }
    }

    /// Given two uniformly sampled random numbers in the range [0, 1], computes
    /// two independent random values with a normal/gaussian distribution
    /// with mean of 0 and standard deviation of 1.
    /// See https://en.wikipedia.org/wiki/Box%E2%80%93Muller_transform
    fn box_muller_transform(u1: f64, u2: f64) -> (f64, f64) {
        let theta = 2.0 * PI * u2;
        let (sin, cos) = theta.sin_cos();
        let r = (-2.0 * u1.ln()).sqrt();

        (r * sin, r * cos)
    }
}

pub trait NormalDistributionRngExt {
    fn next(&mut self, rng: &mut dyn Rng) -> f64;
}

impl NormalDistributionRngExt for NormalDistribution {
    fn next(&mut self, rng: &mut dyn Rng) -> f64 {
        if let Some(v) = self.next_number.take() {
            return v;
        }

        let u1 = rng.between(0.0, 1.0);
        let u2 = rng.between(0.0, 1.0);

        let (z1, z2) = Self::box_muller_transform(u1, u2);
        self.next_number = Some(z2);
        z1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mersenne_twister_test() -> Result<()> {
        let mut rng = MersenneTwisterRng::mt19937();
        rng.seed_u32(1234);

        let data = std::fs::read_to_string(project_path!("testdata/mt19937.txt"))?;

        for (i, line) in data.lines().enumerate() {
            let expected = line.parse::<u32>()?;
            assert_eq!(rng.next_u32(), expected, "Mismatch at index {}", i);
        }

        Ok(())
    }

    #[test]
    fn between_inclusive_test() {
        let mut rng = MersenneTwisterRng::mt19937();
        rng.seed_u32(1234);

        for _ in 0..100 {
            let f = rng.between::<f32>(0.0, 1.0);
            assert!(f >= 0.0 && f < 1.0);
        }

        for _ in 0..100 {
            let f = rng.between::<f64>(0.0, 0.25);
            assert!(f >= 0.0 && f < 0.25);
        }

        let min = 427;
        let max = 674;
        let num_iter = 20000000;
        let mut buckets = [0usize; 247];
        for _ in 0..num_iter {
            let n = rng.between::<i32>(min, max);
            assert!(n >= min && n < max);
            buckets[(n - min) as usize] += 1;
        }

        for bucket in buckets {
            // Ideal value is num_iter / range = ~80971
            // We'll accept a 1% deviation.
            assert!(bucket > 71254 && bucket < 81780, "Bucket is {}", bucket);
        }
    }
}
