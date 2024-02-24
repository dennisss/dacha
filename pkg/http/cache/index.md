# HTTP Cache

File structure:

- `./metadata/`
- `./blobs/`



Schema (in a spanner table):

- HTTPRequests
    - URL (part of primary key)
    - Timestamp (part of primary key) (when we sent out the request)
    - HTTP Method
    - Request headers (excluding cookies)
        - things not mentioned in 'Vary' can also be pruned.
    - Response Status Code
    - Response headers
        - excluding cookies
        - transfer encoding striped
        - Content-Encoding preserved.
    - Response trailers
    - Response body hash (SHA256 if not empty)


Emulating a browser:
- By reactive to cookies
- Use same TLS settings (also inject the fake extension that chrome uses to make it more believable)
    - Ideally these things would track the current browser version if using something like chrome.
    - We can make a fake TLS enabled web page to auto-capture most of the details like HTTP Headers and TLS flags.
- Use same HTTP2 options

Need to be emulating chromium in requests:

```
accept: */*
accept-encoding: gzip, deflate, br
accept-language: en-US,en;q=0.9
cookie: xxxxx
referer: https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching
sec-ch-ua: "Chromium";v="112", "Google Chrome";v="112", "Not:A-Brand";v="99"
sec-ch-ua-mobile: ?0
sec-ch-ua-platform: "Linux"
sec-fetch-dest: empty
sec-fetch-mode: cors
sec-fetch-site: same-origin
user-agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/112.0.0.0 Safari/537.36
```

Things to fully dump

- arxiz
- Google Research PDFs
- RFC website