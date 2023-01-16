



/*
Things to verify when signing a certificate request:
- Must not have the subject equal to the issuer (as this would bypass constraints).
    - Preferably to just re-generate the subject based on the SAN / CN.
    - We should verify the CN is a good DNS name.
*/