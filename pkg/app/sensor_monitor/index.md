

How to log data:
- Key is [metric_name, timestamp]
    - 0x00 0x00
    - Use leading zero varint to compress it.
- Value is 



- Strategy:
    - Go with SGP30 + BMP388
    - 


Turning this into a web app:
- Rust Binary hosting web server on port 8000
    - Serves '/' as an index.html
    - Serves '/assets/'  from a local directory.
        - May require overlays to support the dynamic files.
    - 

    - For now, Typescript/Reacyh