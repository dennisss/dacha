table! {
    cache_machines (id) {
        id -> Int4,
        addr_ip -> Text,
        addr_port -> Int2,
        last_heartbeat -> Timestamptz,
        ready -> Bool,
        alive -> Bool,
        healthy -> Bool,
        hostname -> Text,
    }
}

table! {
    logical_volumes (id) {
        id -> Int4,
        write_enabled -> Bool,
        hash_key -> Int8,
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
        ready -> Bool,
        alive -> Bool,
        healthy -> Bool,
        allocated_space -> Int8,
        total_space -> Int8,
        write_enabled -> Bool,
    }
}

joinable!(photos -> logical_volumes (volume_id));
joinable!(physical_volumes -> logical_volumes (logical_id));
joinable!(physical_volumes -> store_machines (machine_id));

allow_tables_to_appear_in_same_query!(
    cache_machines,
    logical_volumes,
    params,
    photos,
    physical_volumes,
    store_machines,
);
