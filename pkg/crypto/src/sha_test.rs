#![cfg(test)]

// This file contains supplementary test cases for the SHA hashing algorithms.
// For now, this is mainly using the NIST test vectors.

use common::errors::*;
use common::hex;

use crate::hasher::{GetHasherFactory, HasherFactory};
use crate::sha1::*;
use crate::sha224::*;
use crate::sha256::*;
use crate::sha384::*;
use crate::sha512::*;

async fn run_nist_hasher_test(hasher_factory: HasherFactory, paths: &[&'static str]) -> Result<()> {
    let project_dir = file::project_dir();

    for path in paths.iter().cloned() {
        let file = crate::nist::response::ResponseFile::open(project_dir.join(path)).await?;

        for response in file.iter() {
            let response = response?;

            let len = response.fields.get("LEN").unwrap().parse::<usize>()?;
            let msg = hex::decode(response.fields.get("MSG").unwrap())?;
            let md = hex::decode(response.fields.get("MD").unwrap())?;

            // Only byte wise hashing is currently supported.
            assert!((len % 8) == 0);

            let mut hasher = hasher_factory.create();
            hasher.update(&msg[0..(len / 8)]);

            let output = hasher.finish();

            if output != md {
                println!("{}\n{}", hex::encode(&msg), hex::encode(&md));
            }

            assert_eq!(output, md);
        }
    }

    Ok(())
}

#[testcase]
async fn sha1_nist_test() -> Result<()> {
    run_nist_hasher_test(
        SHA1Hasher::factory(),
        &[
            "testdata/nist/shabytetestvectors/SHA1LongMsg.rsp",
            "testdata/nist/shabytetestvectors/SHA1ShortMsg.rsp",
        ],
    )
    .await
}

#[testcase]
async fn sha224_nist_test() -> Result<()> {
    run_nist_hasher_test(
        SHA224Hasher::factory(),
        &[
            "testdata/nist/shabytetestvectors/SHA224LongMsg.rsp",
            "testdata/nist/shabytetestvectors/SHA224ShortMsg.rsp",
        ],
    )
    .await
}

#[testcase]
async fn sha256_nist_test() -> Result<()> {
    run_nist_hasher_test(
        SHA256Hasher::factory(),
        &[
            "testdata/nist/shabytetestvectors/SHA256LongMsg.rsp",
            "testdata/nist/shabytetestvectors/SHA256ShortMsg.rsp",
        ],
    )
    .await
}

#[testcase]
async fn sha384_nist_test() -> Result<()> {
    run_nist_hasher_test(
        SHA384Hasher::factory(),
        &[
            "testdata/nist/shabytetestvectors/SHA384LongMsg.rsp",
            "testdata/nist/shabytetestvectors/SHA384ShortMsg.rsp",
        ],
    )
    .await
}

#[testcase]
async fn sha512_224_nist_test() -> Result<()> {
    run_nist_hasher_test(
        SHA512_224Hasher::factory(),
        &[
            "testdata/nist/shabytetestvectors/SHA512_224LongMsg.rsp",
            "testdata/nist/shabytetestvectors/SHA512_224ShortMsg.rsp",
        ],
    )
    .await
}

#[testcase]
async fn sha512_256_nist_test() -> Result<()> {
    run_nist_hasher_test(
        SHA512_256Hasher::factory(),
        &[
            "testdata/nist/shabytetestvectors/SHA512_256LongMsg.rsp",
            "testdata/nist/shabytetestvectors/SHA512_256ShortMsg.rsp",
        ],
    )
    .await
}

#[testcase]
async fn sha512_nist_test() -> Result<()> {
    run_nist_hasher_test(
        SHA512Hasher::factory(),
        &[
            "testdata/nist/shabytetestvectors/SHA512LongMsg.rsp",
            "testdata/nist/shabytetestvectors/SHA512ShortMsg.rsp",
        ],
    )
    .await
}
