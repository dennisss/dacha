

#include <openssl/rsa.h>
#include <openssl/bio.h>
#include <openssl/pem.h>
#include <openssl/x509.h>
#include <openssl/ssl.h>
#include <cassert>

#include <nice/agent.h>

#include <iostream>
using namespace std;

/*
	TODO: Long term this is probably the faster websocket implementation:
	https://github.com/uNetworking/uWebSockets

	For Golang there is also:
	https://github.com/gobwas/ws

*/

RSA *generate_rsakey() {
	int r;

	// TODO: Also don't forget to do propper seeking before prime generation

	RSA *rsa = RSA_new();
	assert(rsa != NULL);

	BIGNUM *e = BN_new();
	assert(e != NULL);

	r = BN_set_word(e, 65537);
	assert(r == 1);

	r = RSA_generate_key_ex(rsa, 1024, e, NULL);
	assert(r == 1);



	BIO *bio = BIO_new(BIO_s_mem());
	PEM_write_bio_RSAPrivateKey(bio, rsa, NULL, NULL, 0, NULL, NULL);

	int keylen = BIO_pending(bio);
	void *pem_key = malloc(keylen + 1);
	BIO_read(bio, pem_key, keylen);

	printf("%s", pem_key);

	BIO_free_all(bio);
	free(pem_key);

	//RSA_free(rsa);

}


int main(int argc, const char *argv[]) {
	SSL_library_init();
	OPENSSL_init_crypto(0, NULL);
	OpenSSL_add_all_algorithms();

	RSA *rsa = generate_rsakey();
	
	int r;

	X509 *cert = X509_new();
	assert(cert != NULL);


	EVP_PKEY *pkey = EVP_PKEY_new();
	assert(pkey != NULL);

	r = EVP_PKEY_set1_RSA(pkey, rsa);
	//r = EVP_PKEY_assign_RSA(pkey, rsa);
	assert(r == 1);



	r = X509_set_pubkey(cert, pkey);
	cout << "A" << endl;

	assert(r == 1);



	// https://www.openssl.org/docs/manmaster/man3/X509_set_subject_name.html
	//	




	//r = X509_set_version(cert, 0);
	//assert(r == 1);


	
	//X509_gmtime_adj(X509_get_notBefore(cert), 0);


	r = X509_sign(cert, pkey, EVP_sha1());
	assert(r == 1);

	unsigned char buf[1024];
	unsigned int len;
	r = X509_digest(cert, EVP_sha256(), buf, &len);

	cout << len << endl;

}

