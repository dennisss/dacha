# OpenPGP Signature/Message/Key Utilities

Implementation of the formats in https://www.rfc-editor.org/rfc/rfc4880



Keys are in a 'BEGIN PGP PUBLIC KEY BLOCK' block

- Scalars are big endian
- Big integers are: u16 length in bits followed by bytes of the number
    - e.g. [00 01 01] = 1, [00 09 01 FF] = 511
- Key Id = 8 byte scalar
- Text is UTF-8
- Time = 4 byte scalar of seconds since Epoch
- 
