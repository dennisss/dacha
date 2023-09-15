# Google API Authentication

This is a library for setting up RPC/request credentials for communicating with Google APIs.

## Service Accounts

A service account created on GCP is normally stored in some local JSON file containing the account's identity and private key. Using this information, we would:

- Sign a JWT token with a 'scope' claim to request access to an API.
- Send the JWT to the Google OAuth2 HTTPS endpoint to exchange it for a temporary access token
- Use this `access_token` in an HTTP request using a header of the form: `Authorization: Bearer {access_token}`
    - Or for REST-ful APIs, append a query parameter to the url like `?access_token={access_token}`

This flow is documented in:

- https://developers.google.com/identity/protocols/oauth2#serviceaccount
- https://developers.google.com/identity/protocols/oauth2/service-account#authorizingrequests
- https://google.aip.dev/auth/4112


TODO: The base string for the signature is as follows:
`{Base64url encoded header}.{Base64url encoded claim set}`

### Self-signed JWT

To avoid the extra OAuth2 token exchange, most Google APIs allow directly feeding a JWT signed by the client.

This is documented in:

- https://google.aip.dev/auth/4111
- https://developers.google.com/identity/protocols/oauth2/service-account#jwt-auth

Though for some APIs it may not work or requires an additional scopes claim to be set:

- https://www.codejam.info/2022/05/google-cloud-service-account-authorization-without-oauth.html

## gRPC Reference Code

- https://github.com/grpc/grpc/blob/72e791402ff5d9b6dac6075f9df2b53bfa44f6f0/src/core/lib/security/credentials/google_default/google_default_credentials.cc#L352
- https://github.com/grpc/grpc/blob/master/src/core/lib/security/credentials/oauth2/oauth2_credentials.cc#L381
