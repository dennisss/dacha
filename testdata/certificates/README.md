

openssl req -new -subj "/C=US/CN=localhost" -addext "subjectAltName = DNS:localhost" -newkey rsa:2048 -x509 -sha256 -days 1460 -nodes -out testdata/certificates/server.crt -keyout testdata/certificates/server.key

openssl req -new -subj "/C=US/CN=localhost" -addext "subjectAltName = DNS:localhost" -newkey ec:<(openssl ecparam -name prime256v1) -x509 -sha256 -days 1460 -nodes -out testdata/certificates/server-ec.crt -keyout testdata/certificates/server-ec.key
