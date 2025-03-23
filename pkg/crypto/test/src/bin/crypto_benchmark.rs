// Tool for benchmarking crypto functions and also 'validating' that 'probably'
// have constant time behavior of them.
//
// This is essentially an integration test but not defined in #[cfg(test)] as:
// - It is very long running and doesn't test general correctness.
// - Must be executed with a 'release' profile that matches how normal binaries
//   are built to catch any discrepancies introduced by compiler optimizations.

/*
TODO: For benchmarking stuff like AES-GCM, it is also important to benchmark different block sizes

TODO Test suites:
- constant_eq
- SecureBigUint
- SecureMontgomeryModulo
- SecureModulus
- Hash functions

TODOs for making this less noisy:
- Increase 'nice'
- Allocate to a dedicated CPU core.

TODO: Maybe also extend this into a generic fuzzing integration where we have functions with well defined input signatures.
*/

#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};

use common::iter::cartesian_product;
use common::{bool_to_num, errors::*};
use crypto::aead::AuthEncAD;
use crypto::chacha20::{ChaCha20Poly1305, Poly1305};
use crypto::dh::DiffieHellmanFn;
use crypto::elliptic::{EllipticCurveGroup, MontgomeryCurveCodec, MontgomeryCurveGroup};
use crypto::gcm::AesGCM;
use crypto::random::Rng;
use crypto_test::*;
use file::project_path;
use protobuf::Message;

fn constant_eq_test() -> Result<()> {
    let mut gen = TimingLeakTest::new_generator();
    let a_id = gen.add_input(typical_boundary_buffers(100000));
    let b_id = gen.add_input(typical_boundary_buffers(100000));

    TimingLeakTest::new(
        gen,
        |data: &TimingLeakTestGenericTestCase| {
            Ok(crypto::constant_eq(
                data.get_input(a_id),
                data.get_input(b_id),
            ))
        },
        TimingLeakTestOptions {
            num_iterations: 4000,
            num_rounds: 4,
        },
    )
    .run()?;

    Ok(())
}

fn montgomery_group_test<C: MontgomeryCurveCodec>(
    group: MontgomeryCurveGroup<C>,
    input_size: usize,
) {
    println!("public_value()");
    {
        let mut gen = TimingLeakTest::new_generator();
        let secret_id = gen.add_input(typical_boundary_buffers(input_size));

        println!(
            "=> {:?}",
            TimingLeakTest::new(
                gen,
                |data: &TimingLeakTestGenericTestCase| group
                    .public_value(data.get_input(secret_id)),
                TimingLeakTestOptions {
                    num_iterations: 100,
                    num_rounds: 2,
                },
            )
            .run()
        );
    }
    println!("");

    println!("shared_secret()");
    {
        let mut gen = TimingLeakTest::new_generator();
        let secret_id = gen.add_input(typical_boundary_buffers(input_size));
        let public_id = gen.add_input(typical_boundary_buffers(input_size));

        println!(
            "=> {:?}",
            TimingLeakTest::new(
                gen,
                |data: &TimingLeakTestGenericTestCase| group
                    .shared_secret(data.get_input(public_id), data.get_input(secret_id)),
                TimingLeakTestOptions {
                    num_iterations: 100,
                    num_rounds: 3,
                },
            )
            .run()
        );
    }
    println!("");
}

fn poly1305_test() {
    let mut gen = TimingLeakTest::new_generator();

    let key_id = gen.add_input(typical_boundary_buffers(32));
    let data_id = gen.add_input(typical_boundary_buffers(4096));

    println!(
        "=> {:?}",
        TimingLeakTest::new(
            gen,
            |data: &TimingLeakTestGenericTestCase| {
                let mut poly = Poly1305::new(data.get_input(key_id));
                poly.update(data.get_input(data_id), false);
                Ok(poly.finish())
            },
            TimingLeakTestOptions {
                num_iterations: 10000,
                num_rounds: 3,
            },
        )
        .run()
    );

    println!("");
}

async fn signature_benchmark(typ: crypto::x509::PrivateKeyType) -> Result<()> {
    println!("{:?}", typ);

    let pkey = crypto::x509::PrivateKey::generate(typ).await?;

    let signature_algorithm = pkey.default_signature_algorithm();
    let constraints = crypto::x509::SignatureKeyConstraints::default();

    {
        let start = Instant::now();
        let mut n = 0;
        for _ in 0..100 {
            n += pkey
                .create_signature(&[1, 2, 3], &signature_algorithm, &constraints)
                .await?
                .len();
        }
        let end = Instant::now();

        assert!(n > 0);

        eprintln!("create_signature: {:?}", (end - start) / 100);
    }

    {
        let signature = pkey
            .create_signature(&[1, 2, 3], &signature_algorithm, &constraints)
            .await?;

        let public_key = pkey.public_key()?;

        let start = Instant::now();
        let mut n = 0;
        for _ in 0..100 {
            n += bool_to_num!(public_key.verify_signature(
                &[1, 2, 3],
                &signature,
                &signature_algorithm,
                &constraints
            )?);
        }
        let end = Instant::now();

        assert!(n > 0);

        eprintln!("verify_signature: {:?}", (end - start) / 100);
    }

    Ok(())
}

async fn key_exchange_benchmark(f: &dyn DiffieHellmanFn) -> Result<()> {
    let start = Instant::now();

    let mut n = 0;
    for _ in 0..100 {
        let secret = f.secret_value().await?;

        let public = f.public_value(&secret)?;

        let out = f.shared_secret(&public, &secret)?;

        n += out.len();
    }

    let end = Instant::now();

    assert!(n > 0);

    eprintln!("key_exchange: {:?}", (end - start) / 100);

    Ok(())
}

async fn aead_speed_benchmark(aead: &dyn AuthEncAD) -> Result<()> {
    let aes_gcm = AesGCM::aes128();

    let mut key = vec![0u8; aead.key_size()];
    crypto::random::clocked_rng().generate_bytes(&mut key);

    let mut nonce = vec![0u8; aead.nonce_range().1];
    crypto::random::clocked_rng().generate_bytes(&mut nonce);

    let mut data = vec![0u8; 1 * 1024 * 1024];
    crypto::random::clocked_rng().generate_bytes(&mut data);

    let mut tmp = vec![];
    let mut tmp2 = vec![];

    let start = Instant::now();
    let num_iters = 200;
    for _ in 0..num_iters {
        tmp.clear();
        aead.encrypt(&key, &nonce, &data, &[], &mut tmp);

        tmp2.clear();
        aead.decrypt(&key, &nonce, &tmp, &[], &mut tmp2)?;
    }
    let end = Instant::now();

    println!("=> Time per MiB: {:?}", (end - start) / num_iters);

    Ok(())
}

/*
Want performance numbers on the whole Chacha20 + Poly1305 AEAD flow.

*/

#[executor_main]
async fn main() -> Result<()> {
    // TODO: Wait for this thing to start profiling.
    let profile = executor::spawn(perf::profile_self(Duration::from_secs(10)));

    // println!("constant_eq:");
    // println!("=> {:?}", constant_eq_test());
    // println!("");

    // println!("x25519:");
    // montgomery_group_test(MontgomeryCurveGroup::x25519(), 32);

    // println!("x448:");
    // montgomery_group_test(MontgomeryCurveGroup::x448(), 56);

    // println!("poly1305:");
    // poly1305_test();

    // signature_benchmark(crypto::x509::PrivateKeyType::Ed25519).await?;
    // key_exchange_benchmark(&MontgomeryCurveGroup::x25519()).await?;

    // signature_benchmark(crypto::x509::PrivateKeyType::ECDSA_SECP256R1).await?;
    // key_exchange_benchmark(&EllipticCurveGroup::secp256r1()).await?;

    println!("aes_gcm_128");
    aead_speed_benchmark(&AesGCM::aes128()).await?;
    println!("chacha20_poly1305");
    aead_speed_benchmark(&ChaCha20Poly1305::new()).await?;

    let profile = profile.join().await?;
    file::write(project_path!("perf.pb"), profile.serialize()?).await?;

    /*
    secp256r1
    GCM
    AES
    ChaCha20
    */

    Ok(())
}
