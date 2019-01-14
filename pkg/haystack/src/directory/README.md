
Directory
=========

Maintains all volume, machine, health, and photo mappings. Backed by a separate relational database. Therefore this code doesn't have any code to run, but rather serves mainly as an internal API to the other layers that do use this.


Responsibilities according to the original research paper:

1. "First, it provides a mapping from logical volumes to physical volumes."
	- This is implemented as a database table called `physical_volumes` that is externally queried and updated

2. "Second, the Directory load balances writes across logical volumes and reads across physical volumes"
	- Currently this is implemented as a random choice between volumes and is tightly coupled with the client implementation

3. "Third, the Directory determines whether a photo request should be handled by the CDN or by the Cache"
	- TODO

4. "Fourth, the Directory identifies those logical volumes that are read-only either because of op- erational reasons or because those volumes have reached their storage capacity"
	- This like #1 is implemented purely as a set of database tables that are updated by the stores and by pitch fork


