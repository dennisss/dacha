# Cryptography Algorithms / Primitives

## Well Supported Algorithms

These are the algorithms which are well supported by this library. Where well supported means:
- Unit-tested
- Constant time operation
- Performance optimized

Checksums:
- CRC32C : Heapless, SSE4.2, ARM64 native instructions.
- CRC32 : Heapless, LUT optimized.
- CRC16 : Heapless, LUT optimized.
- Adler32 : Heapless

Block Ciphers
- AES-128/256 : x64 AES-NI native instructions
  - TODO: Make heapless.
- 

Network Protocols:
- TLS 1.2/1.3
  -

TODO: Need tests with corrupt signatures to verify that we don't simply always report signatures as always good.

Critical Algorithms for TLS:

        supported_cipher_suites: vec![
            // SHOULD implement
            CipherSuite::TLS_CHACHA20_POLY1305_SHA256,
            // MUST implement
            CipherSuite::TLS_AES_128_GCM_SHA256,
            // SHOULD implement
            CipherSuite::TLS_AES_256_GCM_SHA384,
            // TLS 1.2 Only
            CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
            CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
            CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
        ],
        supported_groups: vec![
            // SHOULD support
            NamedGroup::x25519,
            // MUST implement
            NamedGroup::secp256r1,
            // optional
            NamedGroup::secp384r1,
        ],
        supported_signature_algorithms: vec![
        // TODO: Also add ed2519

            // TLS 1.3: These three are the minimum required set to implement.
            SignatureScheme::ecdsa_secp256r1_sha256,
            SignatureScheme::rsa_pss_rsae_sha256,
            SignatureScheme::rsa_pkcs1_sha256,
            // Extra to allow old TLS 1.2 servers to have a decent fallback.
            SignatureScheme::rsa_pkcs1_sha384,
            SignatureScheme::rsa_pkcs1_sha512,


Immediate TODO:
- MUST verify constant time operation of SecureBigUint on different architectures.
- Ed25519 / X25519 as constant time and heapless
  - Essential for nordic communication.
- For prime256v1, we need to validate curve points are actually valid.
- We should have a constant benchmark going on signature verification time (and record TLS handshake time).
- AES-GCM constant time implementation
- TLS 1.2/1.3 unit tests



Things we need to support right now:
- 




## Other


TODO: See also this module:
https://docs.rs/digest/0.8.1/digest/trait.Digest.html

https://github.com/RustCrypto/hashes

https://docs.rs/hmac/0.7.1/hmac/#structs


Wikipedia still uses 1.2:
The connection to this site is encrypted and authenticated using TLS 1.2, ECDHE_ECDSA with X25519, and CHACHA20_POLY1305.


 The five cryptographic operations -- digital signing, stream cipher
   encryption, block cipher encryption, authenticated encryption with
   additional data (AEAD) encryption, and public key encryption -- are
   designated digitally-signed, stream-ciphered, block-ciphered, aead-
   ciphered, and public-key-encrypted, respectively.


TODO: It is interesting that a phone number can be used to get a person's name
if they are registered on hangouts/google.

TODO: 
- Algorithmic complexity attacks analysis: https://www.youtube.com/watch?v=UdTpa-n9L-g

- zxcvbn


How BorringSSL implements the NIST format:
- https://github.com/google/boringssl/blob/94b477cea5057d9372984a311aba9276f737f748/crypto/test/file_test.h
- https://boringssl.googlesource.com/boringssl/+/refs/tags/fips-20180730/fipstools/cavp_rsa2_siggen_test.cc

More info on ACVP
- https://www.wolfssl.com/what-is-acvp/
- 

Reference for finding test vectors for algoithms:
-  https://cryptography.io/en/latest/development/test-vectors/#sources

Relevant RFCs

- [RFC 8017: PKCS #1 v2.2](https://datatracker.ietf.org/doc/html/rfc8017)
- [RFC 8439: ChaCha20 & Poly1305](https://datatracker.ietf.org/doc/html/rfc8439)
- [RFC 5116: AEAD](https://datatracker.ietf.org/doc/html/rfc5116)
- [RFC 7301: ALPN](https://datatracker.ietf.org/doc/html/rfc7301)

Encrypted Client Hello
- https://datatracker.ietf.org/doc/html/draft-ietf-tls-esni


https://datatracker.ietf.org/doc/html/rfc5756

How to generate secure RSA prime numbers:
- https://crypto.stackexchange.com/questions/71/how-can-i-generate-large-prime-numbers-for-rsa
- https://csrc.nist.gov/csrc/media/publications/fips/186/3/archive/2009-06-25/documents/fips_186-3.pdf
- https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.186-4.pdf
- https://en.wikipedia.org/wiki/RSA_(cryptosystem)#Key_generation
- https://en.wikipedia.org/wiki/Fermat_primality_test
- https://en.wikipedia.org/wiki/Miller%E2%80%93Rabin_primality_test