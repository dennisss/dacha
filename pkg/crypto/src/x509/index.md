

Relevant RFCs:
- https://datatracker.ietf.org/doc/html/rfc5280
- RFC 7093 Section 2(1)
    - Suggests generating key identifiers based on SHA256



/*
Things to verify when signing a certificate request:
- Must not have the subject equal to the issuer (as this would bypass constraints).
    - Preferably to just re-generate the subject based on the SAN / CN.
    - We should verify the CN is a good DNS name.
*/