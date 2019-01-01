table! {
    cache_machines (id) {
        id -> Int4,
        addr_ip -> Text,
        addr_port -> Int2,
        last_heartbeat -> Timestamptz,
        ready -> Bool,
        hostname -> Text,
    }
}

table! {
    logical_volumes (id) {
        id -> Int4,
        num_needles -> Int8,
        used_space -> Int8,
        allocated_space -> Int8,
        write_enabled -> Bool,
        hash_key -> Int8,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

table! {
    params (id) {
        id -> Int4,
        value -> Bytea,
    }
}

table! {
    photos (id) {
        id -> Int8,
        volume_id -> Int4,
        cookie -> Bytea,
    }
}

table! {
    physical_volumes (logical_id, machine_id) {
        logical_id -> Int4,
        machine_id -> Int4,
    }
}

table! {
    store_machines (id) {
        id -> Int4,
        addr_ip -> Text,
        addr_port -> Int2,
        last_heartbeat -> Timestamptz,
        allocated_space -> Int8,
        total_space -> Int8,
        reclaimed_space -> Int8,
        write_enabled -> Bool,
        dirty -> Bool,
    }
}

allow_tables_to_appear_in_same_query!(
    cache_machines,
    logical_volumes,
    params,
    photos,
    physical_volumes,
    store_machines,
);
