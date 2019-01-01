-- Your SQL goes here

CREATE TABLE params (
	id INT PRIMARY KEY,
	value BYTEA NOT NULL 
);

CREATE TABLE store_machines (
	id SERIAL PRIMARY KEY,
	addr_ip TEXT NOT NULL,
	addr_port SMALLINT NOT NULL,
	last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	
	allocated_space BIGINT NOT NULL DEFAULT 0,
	total_space BIGINT NOT NULL DEFAULT 0,
	reclaimed_space BIGINT NOT NULL DEFAULT 0,
	write_enabled BOOLEAN NOT NULL DEFAULT FALSE,
	dirty BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE cache_machines (
	id SERIAL PRIMARY KEY,
	addr_ip TEXT NOT NULL,
	addr_port SMALLINT NOT NULL,
	last_heartbeat TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	ready BOOL NOT NULL DEFAULT FALSE,
	hostname TEXT NOT NULL
);

CREATE TABLE logical_volumes (
	id SERIAL PRIMARY KEY,
	num_needles BIGINT NOT NULL DEFAULT 0,
	used_space BIGINT NOT NULL DEFAULT 0,
	allocated_space BIGINT NOT NULL DEFAULT 0,
	write_enabled BOOL NOT NULL DEFAULT FALSE,
	hash_key BIGINT NOT NULL,
	created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

	-- https://x-team.com/blog/automatic-timestamps-with-postgresql/
	CREATE OR REPLACE FUNCTION trigger_set_timestamp()
	RETURNS TRIGGER AS $$
	BEGIN
		NEW.updated_at = NOW();
		RETURN NEW;
	END;
	$$ LANGUAGE plpgsql;

	CREATE TRIGGER set_timestamp
	BEFORE UPDATE ON logical_volumes
	FOR EACH ROW
	EXECUTE PROCEDURE trigger_set_timestamp();


CREATE TABLE physical_volumes (
	logical_id INT NOT NULL,
	machine_id INT NOT NULL,
	PRIMARY KEY(logical_id, machine_id)
);

CREATE TABLE photos (
	id BIGINT PRIMARY KEY,
	volume_id INT NOT NULL,
	cookie BYTEA NOT NULL
);
