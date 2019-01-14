HTTP API
========

Both the cache machines and the store machines expose all of their functionality via an HTTP API

- The Facebook paper references urls of the form: `http://⟨CDN⟩/⟨Cache⟩/⟨Machine id⟩/⟨Logical volume, Photo⟩` for accessing methods via the CDN
	- While this schema is not completely descriptive we do follow it reasonably closely and in general API routes to the store machines will be front-truncated versions of the corresponding cache paths

- Cookies are always base64 encoded 128bit byte arrays
- Keys/ids appearing in urls are always expected as strings in human readable base 10
- Every API request should have a `Host` header on it that contains the type and id of the machine
	- Because port and ip assignments can change over restarts, we enforce that the id in the Host header for every request matches the machine it is going to in order to prevent scenarios such as uploading to the wrong machine
	- Currently the first three segments of the below host names are hardcoded into the code and are expected in that form
	- For caches, it should look like `[cache_id].cache.hay.[some.domain.com]`
	- For stores, it should look like `[store_id].store.hay.[some.domain.com]`

- Both the store and cache layers support `ETag`, `If-None-Match` response/request headers
- Any uploads to these servers must have a well specified `Content-Length` header in the request


Cache API
---------

- GET `http://[host]/`
	- Prints out JSON data about the current cache including utilization information

- GET `http://[host]/:store_id/:logical_id/:photo_key/:alt_key/:cookie`
	- Reads a photo from the cache or proxies the request to the specified store on cache miss

- POST `http://[host]/:store_id/:logical_id/:photo_key/:alt_key/:cookie`
	- Uploads a single photo given in the request body to a set of store machines in parallel
	- For this request `:store_id` should be set to a `-` separated list of store_id numbers or to `-` in order to automatically select all stores containing the given logical volume.
	- While uploads can be performed directly to store machines, this operation will simulataneously fill the cache while performing the upload


Store API
---------

- GET `http://[host]/`
	- Prints out the list of all volumes on this machine with utilization information

- POST `http://[host]/:logical_id`
	- Creates a new physical volume (or succeeds with a no-op if it already exists)

- PATCH `http://[host]/:logical_id`
	- Batch upload many needles to a single volume
	- Will flush the volume to disk only after all have been saved
	- Body of the request should consist of multiple files prefixed by a header with binary data:
		- `[key][alt_key][cookie][size]`
- GET `http://[host]/:logical_id/:photo_key/:alt_key`
	- Reads the contents of a single photo from the store WITHOUT cookie authentication
	- NOTE: This 

- DELETE `http://[host]/:logical_id/:photo_key`
	- Marks all needles associated with a single photo key as deleted

- GET `http://[host]/:logical_id/:photo_key/:alt_key/:cookie`
	- Reads the contents of a single photo from the store performing cookie authentication

- DELETE `http://[host]/:logical_id/:photo_key/:alt_key`
	- Marks the needle as deleted

- POST `http://[host]/:logical_id/:photo_key/:alt_key/:cookie`
	- Upload a single needle with the given keys and cookie
	- This will append it to the end of the volume and will override any previously existing needle on sequential reads

