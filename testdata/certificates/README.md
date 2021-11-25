

openssl req -new -newkey rsa:2048 -x509 -sha256 -days 1460 -nodes -out testdata/certificates/server.crt -keyout testdata/certificates/server.key

openssl req -new -newkey ec:<(openssl ecparam -name prime256v1) -x509 -sha256 -days 1460 -nodes -out testdata/certificates/server-ec.crt -keyout testdata/certificates/server-ec.key

Country Name (2 letter code) [AU]:US
State or Province Name (full name) [Some-State]:California
Locality Name (eg, city) []:
Organization Name (eg, company) [Internet Widgits Pty Ltd]:Dacha
Organizational Unit Name (eg, section) []:Test
Common Name (e.g. server FQDN or YOUR name) []:localhost
Email Address []:
