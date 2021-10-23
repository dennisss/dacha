

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