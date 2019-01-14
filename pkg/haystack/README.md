Finding a needle in Haystack: Facebook’s photo storage
======================================================

This is an implementation of a distributed object/photo store as described by Facebook's paper named the same as the title above

This is an implementation that tries to stay true to the original implementation as much as possible


Setup
-----

0. Build using `cargo build`. All further instructions will reference the executable at `./target/debug/hay`

1. Setup a PostgreSQL database
	- This database will be used as the `directory` for the system and will store machine configuration information and mappings for where to find each photo
	- Follow some other guide for getting one up and running on the current or networked machine
	- Export the `HAYSTACK_DB` environment variable to reflect your setup
		- The default value is `postgres://localhost/haystack` and will use a database named `haystack` on a locally running database instance

2. Initialize the cluster
	- Run `hay init`
	- The above command will create the database and setup all needed tables

3. Start up store machines
	- Run `hay store -f DATA_FOLDER -p STORE_PORT` for each isolated store you want to create
		- 3 separate store instances will serve as replicas for each volume/photo
	- Each store instance must run on unique ports and in unique folders 
	- On restarts, it is only necessary for the contents of the data folder to be preserved

4. Start up cache machines
	- Run `hay cache -p CACHE_PORT`

5. Start a single pitch-fork instance
	- TODO


Usage
-----

Currently the easiest way to interact with the store is via the CLI client:

- Creating/upload a new photo
	- `hay client upload 1 file.png [2 file.png [3 file3.png ...]]`
		- This will create a new photo from an existing file (`file.png`) and will upload it with alt_key 1
		- Providing multiple pairs of alt_key/filenames will upload multiple files under the same id/key with the different alt_keys given
		- NOTE: Currently all alt_keys for a single photo must be uploaded all at once
		- The key/id of the created photo will be printed to stdout on success with the format

	- stdout json format: `{"id":1}`

- Read a photo 
	- `hay client read-url [key] [alt_key]`
		- Upon success this will print `{"url":"http://..."}`
	- The url from the above command can be fetched in order to get the contents of the photo

- Update/overwrite a photo
	`hay client upload --key [id] [... same arguments as for uploading ...]`
		- The above command will overwrite an existing photo. It differs from the regular upload command with the addition of a `--key` parameter that forces the photo id used
	- NOTE: When re-uploading, all alt_keys should be provided at once and it is currently inconsistent to only update some of the alt_keys

- Delete a photo
	- `hay client delete [key]`
		- This will delete a photo along with ALL of its `alt_key` components


TODOs
-----
- Haystress
- Better support for multi-machine networks and a CDN routing configuration
- Sharding of caches/stores/pitch-forks per region
	- Facebooks implementation uses region information to ensure that at least one replica is chosen in a remote location
- Implement pitch-fork
- Compaction
- Batch-uploads
- Cache all machine-configurations from the directory in memory on each machine
- More production ready access control to 
- HTTPS

- Testing against Randomio
	- https://web.archive.org/web/20090506102156/http://members.optusnet.com.au/clausen/ideas/randomio/index.html
	- 


Production Notes
----------------
- Both the store and cache machines should be shutdown gracefully using a SIGINT whenever possible
- Attaching more space to a store currently requires restarting the store process running on that machine
- For optimal performance, only start one store process per RAID/disk configuration / machine.
- Only the cache machines should be publically accessible (although some operations on them likely still need to be well filtered beyond what we do now as we do allow raw uploading from a cache machine)
- In the presense of updates to an existing photo key, caches may return stale responses to old versions until the maximum cache age expires
- New uploads and updates are not atomic and may result in dangling needles not being used by any current photo

TODO: Would be nice to just have a set of Kubernetes configs for this (or a helm package encapsulating all of it)


Design Invariants
-----------------

- The cookie for all alt keys under a single photo key are the same
	- This is consistent with how facebook does it and helps reduce the amount of storage needed in the directory

- The cookie for overwritten versions of the same photo key is the same as that for old versions
	- Because switching of photo/needle versions is not atomic and very much subject to caching effects, changing the cookie may cause many reads to suddenly fail if not successful or if old clients are still requesting files

- All alternative keys for a single photo key should be uploaded all at once (especially during modifications)
	- Because we currently assume that all needles for a single photo key exist on the same logical volume, overwriting them with a partial set of new alt keys may change the logical volume for all of the alt keys and thus silently forget about previously uploaded alt keys

- Once a photo is marked as deleted, it can not be undeleted


Other open source implementations
---------------------------------
- https://github.com/chrislusf/seaweedfs
- https://github.com/hackeryoung/haystack
- https://github.com/Topface/backpack
