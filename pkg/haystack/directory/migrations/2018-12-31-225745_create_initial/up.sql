-- Your SQL goes here

CREATE TABLE params (
	id INT PRIMARY KEY CHECK (id > 0),
	value BYTEA NOT NULL 
);

CREATE TABLE store_machines (
	id SERIAL PRIMARY KEY CHECK (id > 0),
	addr_ip TEXT NOT NULL,
	addr_port SMALLINT NOT NULL CHECK (addr_port > 0),
	last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	ready BOOLEAN NOT NULL DEFAULT FALSE,
	alive BOOLEAN NOT NULL DEFAULT FALSE,
	healthy BOOLEAN NOT NULL DEFAULT FALSE,
	allocated_space BIGINT NOT NULL DEFAULT 0 CHECK (allocated_space >= 0),
	total_space BIGINT NOT NULL DEFAULT 0 CHECK (total_space >= 0),
	write_enabled BOOLEAN NOT NULL DEFAULT FALSE,
	CHECK (allocated_space <= total_space)
);

CREATE TABLE cache_machines (
	id SERIAL PRIMARY KEY CHECK (id > 0),
	addr_ip TEXT NOT NULL,
	addr_port SMALLINT NOT NULL CHECK (addr_port > 0),
	last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	ready BOOL NOT NULL DEFAULT FALSE,
	alive BOOL NOT NULL DEFAULT FALSE,
	healthy BOOL NOT NULL DEFAULT FALSE,
	hostname TEXT NOT NULL
);

CREATE TABLE logical_volumes (
	id SERIAL PRIMARY KEY CHECK (id > 0),
	write_enabled BOOL NOT NULL DEFAULT FALSE,
	hash_key BIGINT NOT NULL -- This will be transmutted into a u64 preserving the bitwise value 
);

CREATE TABLE physical_volumes (
	logical_id INT NOT NULL REFERENCES logical_volumes (id),
	machine_id INT NOT NULL REFERENCES store_machines (id),
	PRIMARY KEY(logical_id, machine_id)
);

CREATE TABLE photos (
	id BIGSERIAL PRIMARY KEY,
	volume_id INT NOT NULL REFERENCES logical_volumes (id),
	cookie BYTEA NOT NULL CHECK (LENGTH(cookie) = 16 )
);
