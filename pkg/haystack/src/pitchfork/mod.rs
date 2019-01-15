

/*
	TODO: Must actively react to when stores get marked as ready and us that as an indicator to try to mark that store as active

	TODO: Probably also health check the cache macines as well

	This has many responsibilities:

	In summary
	- Unhealthy writeable volumes must be taken down
	- Unhealthy ready machines must be taken down
	- Underreplicated read-only volumes just be re-replicated
		- TODO: Replicacion should re-check date integrity to ensure that we aren't replicating garbage
	- Read-only volumes that are healthy and spacious should get re-flagged as writeable 
	- Stray volumes on machines that have no db entry(s) must be cleaned up

	----------

	- For all machines
		- Try to connect to them and probe random files on disk
		- If a machine contains a volume that it is not allocated towards
			- Wait some amount of time for the machine that originally created it to settle
			- Atomically delete the volume under the assumption that no photos 
			- Remove it from the machine (to allow more volumes to be created)
				- TODO: We should probably have the initial machine acquire a liveness lock with a locking system to be able to create the volume

		- If not healthy, mark all associated volumes are read-only
		- NOTE: If a machine is not associated with any 

	- For all read-only volumes
		- Step 1: verify that enough replicas exist for it
			- If missing replicas, wait for some amount of time for them to come online
			- If they don't come online, create a new replica built through replication
				- Once fully replicated, add it to the machines set and remove the
				- TODO: Make sure no other pitch fork services try to delete it while we are replicating

		- Step 2: Check how much free space there is on each machine for the volume
			- If there is none on all machines, skip the rest of these steps and go to the next volume
			- NOTE: This may change if compactions occur
			- Obviously we don't need to do anything if they report no available compactions possible

	For all writeable volumes
		- 


*/