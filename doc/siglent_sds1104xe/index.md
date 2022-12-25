
Rooting based on:
- https://www.makermatrix.com/blog/hacking-the-siglent-1104x-e-oscilloscope/

In SCPI interface

Request: `SCOPEID?`

Response: `SCOPE_ID [id]`

Enter information into `third_party/siglent_hack/keygen.py` and run the script

For each of the codes for `200M`, `MSO`, `AWG`, `WIFI`, perform:

- Run `MCBD <code>`

- Then run `PRBD?`

Finally power cycle the device.