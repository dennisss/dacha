/*
	TODO: Most of this block comment is yet to be implemented

	This is an overarching integration test
	- It will start:
		- 4 store servers in tmp dirs
		- 2 cache servers
	
	- We will assume that repo/data/picsum has a set of raw images 

	- It will then randomly switch between uploading new images

	- Finally we will mass-read random images and verify that the distribution between the two cache servers is well balanced

	Single store tests
	------------------
	- Verify all basic operations directly on a store via HTTP and maybe a few volumes on it
	- Most of the checksum and screwing around stuff can be tested on a single store basis
	
	- Ideally when creating a volume, we should specify the allocated size at that exact moment for better flexibility
	- To be stored in the volumes index on the whole machine


	Basic things
	------------
	- Reading with the incorrect cookie (or malformed cookie) should fail (when requested directly to the store)
	- Reading with correct cookie should suceed
	- Reading a needle with unknown alt-key/volume/etc. should 404
	- When reading a photo successfully from the cache and then requesting the same photo with a bad cookie to the cache, it should fail
		- Basically must verify that cookie checks apply to both direct store requests and requests from cache
	- Photos larger than the max caching size should not get put into the cache

	- Fuzzing malformed/random urls sent to the cache


	Failure cases to test:
	----------------------
	- Killing one store server
		- should cause uploads to fail
		- Reads should still work
	- Killing one cache server
	- Maxing out a store server 

	Other cases:
	------------
	- Two stores should not be startable in the same directory
	- Calling a store with the wrong hostname should fail
	- Graceful restarts:
		- Stopping and restarting a haystore should succeed
		- Deleting the index file for a store's volumes should be able to gracefully recreate the exact same file on restart
	- Randomly corrupting bytes in a store volume should cause reads to gracefully fail due to corruption
		^ Should ideally never crash

	- A partially complete physical volume index should be recoverable
	- A partially written volume (not fully flushed to disk) should be recoverable be truncating off bad data at the end of it

	- The store should fail to open randomly fuzzed files

	- Volume allocated space is respected
		- Upload consistently to a single volume
		- Once it hits the allocated storage size it should stop accepting (and reject further writes)
			- Should also get marked as read-only
		-  

	- 

*/

import { execSync, exec, ChildProcess, ExecSyncOptionsWithStringEncoding, ExecSyncOptions } from 'child_process';
import path from 'path';
import fs from 'fs';
import tmp from 'tmp'
import axios from 'axios';
import child_process from 'child_process';

const ROOT_DIR = path.resolve(__dirname, '../../..');


function pause(time: number) {
	return new Promise((res) => {
		setTimeout(res, time);
	});
}

function waitcp(cp: ChildProcess): Promise<number> {
	return new Promise((res) => {
		cp.on('exit', (code) => {
			res(code || -1);
		})
	})
}

describe('Haystack', () => {

	const TEST_DBNAME = 'haystack_test';
	const TEST_DBURL = `postgres://localhost/${TEST_DBNAME}`;

	let processes: ChildProcess[] = [];

	process.on('SIGINT', () => {
		for(var i = 0; i < processes.length; i++) {
			processes[i].kill('SIGTERM');
		}

		process.exit(0);
	});

	function run(args: string): ChildProcess {
		let cp = child_process.spawn('./target/debug/hay', args.split(/\s+/), {
			cwd: ROOT_DIR,
			env: {
				HAYSTACK_DB: TEST_DBURL
			},
			stdio: ['ignore', 'inherit', 'inherit'],
			detached: true
		});

		processes.push(cp);
		cp.on('exit', () => {
			processes.splice(processes.indexOf(cp), 1);
		});

		return cp;
	}


	let tmpDir: tmp.SynchrounousResult;

	before(async () => {
		//execSync('cargo build', { cwd: ROOT_DIR, stdio: 'inherit' });
	});



	beforeEach(async function() {
		this.timeout(8000);

		tmpDir = tmp.dirSync({ prefix: 'hay-' });
		console.log('Tmp dir: ' + tmpDir.name);

		// Wipe and create a new database
		try {
			execSync(`psql postgres -c "DROP DATABASE ${TEST_DBNAME};"`);
		}
		catch(err) { /* Will fail on non-existent DB */ }
		execSync(`psql postgres -c "CREATE DATABASE ${TEST_DBNAME}"`);

		execSync(`${process.env.HOME}/.cargo/bin/diesel migration run`, {
			cwd: path.join(ROOT_DIR, 'pkg/haystack/src/directory'), env: {
				DATABASE_URL: TEST_DBURL
			},
			stdio: 'inherit'
		});


		for(let i = 0; i < 2; i++) {
			let port = 5000 + i;
			run(`cache -p ${port}`);
			
			await pause(50);
		}


		for(let i = 0; i < 4; i++) {
			let dir = path.join(tmpDir.name, `hay${i}`); fs.mkdirSync(dir);
			let port = 4000 + i;
			run(`store -f ${dir} -p ${port}`);

			// Mainly to allow each process to take a sequential machine id to make things easier
			await pause(50);
		}

		// Block until all stores are responding to us
		for(let i = 0; i < 4; i++) {
			while(true) {
				try {
					let res = await axios.get(`http://127.0.0.1:${4000 + i}`, { headers: { 'Host': `${i + 1}.store.hay` } });

					// Keep trying until a volume is ready
					// TODO: One current issue is that this amount of time is pretty random and relative to the heartbeat duration so this may take a while
					if(res.data.length > 0) {
						break;
					}
				}
				// All errors should only be from not being able to connect yet
				catch(err) { if(err.response) { throw err; } }
				
				await pause(100);
			}
		}

		// Must wait until all machines have done another heartbeat after creating their volumes so that they get marked as write-enabled
		await pause(500);
	});

	afterEach(async () => {
		for(let cp of processes) {
			cp.kill();
		}

		// TODO: Block until completely exited? (also what if they are already exited)

		execSync(`rm -rf ${tmpDir.name}/*`);
		tmpDir.removeCallback();
	});

	it('can start up and upload to machines', async function() {
		this.timeout(20000);


		let inputDir = path.join(ROOT_DIR, 'data/picsum');
		let files = fs.readdirSync(inputDir);

		// TODO: Use sharp for resizing to a 4 level scale pyramid for now based on the original size

		for(var i = 0; i < files.length; i++) {
			await waitcp(run(`client upload 0 ${path.join(inputDir, files[i])}`));
		}

		await waitcp(run(`client read-url 1 0`));

	});

});
