# JSON Web Token Library

Based on https://datatracker.ietf.org/doc/html/rfc7519

TODO: Rename this to the JOSE crate.

Algorithms:
- https://www.rfc-editor.org/rfc/rfc7518.html

JWK:
- https://www.rfc-editor.org/rfc/rfc7517.html


JWK Registry:
- https://www.iana.org/assignments/jose/jose.xhtml#web-key-types


JWK For EcDSA keys
- https://www.rfc-editor.org/rfc/rfc8037.html#appendix-A.2


JWS format:
- https://datatracker.ietf.org/doc/html/rfc7515#section-5.1

 {"kty":"OKP","crv":"Ed25519",
   "x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"}

JWK Thumbprint
- https://datatracker.ietf.org/doc/html/rfc7638