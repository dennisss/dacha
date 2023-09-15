
Generated using:

```
openssl req -nodes -new -subj "/C=US/CN=*.example.com" \
    -addext "subjectAltName = DNS:hello.org" \
    -newkey ed25519 -keyout testdata/x509/csr/private.key -out testdata/x509/csr/request.csr

```
