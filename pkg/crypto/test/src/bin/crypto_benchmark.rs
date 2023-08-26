// Tool for benchmarking crypto functions and also validating constant time
// behavior of them.
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

extern crate common;
extern crate crypto;

use common::errors::*;
use common::iter::cartesian_product;
use crypto::chacha20::Poly1305;
use crypto::dh::DiffieHellmanFn;
use crypto::elliptic::{MontgomeryCurveCodec, MontgomeryCurveGroup};
use crypto::test::*;

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

fn main() -> Result<()> {
    // println!("constant_eq:");
    // println!("=> {:?}", constant_eq_test());
    // println!("");

    // println!("x25519:");
    // montgomery_group_test(MontgomeryCurveGroup::x25519(), 32);

    // println!("x448:");
    // montgomery_group_test(MontgomeryCurveGroup::x448(), 56);

    println!("poly1305:");
    poly1305_test();

    /*
    secp256r1
    GCM
    AES
    ChaCha20
    */

    Ok(())
}
