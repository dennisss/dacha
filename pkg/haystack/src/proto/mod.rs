#![allow(dead_code, non_snake_case)]

pub mod service {
    include!(concat!(env!("OUT_DIR"), "/src/proto/service.rs"));
}

pub mod config {
    include!(concat!(env!("OUT_DIR"), "/src/proto/config.rs"));

    impl Config {
        pub fn recommended() -> Self {
            let mut inst = Config::default();
            inst.set_store(StoreConfig::recommended());
            inst.set_cache(CacheConfig::recommended());
            inst
        }
    }

    impl StoreConfig {
        pub fn recommended() -> Self {
            StoreConfig {
                num_replicas: 3,
                block_size: 64,
                allocation_size: 100 * 1024 * 1024, // 100MB for testing
                allocation_reserved: 2,
                preallocate_size: 1 * 1024 * 1024, // 1MB for testing
                space: 1024 * 1024 * 1024,         // 1GB
                heartbeat_interval: 10000,         // Heartbeat send every 10 seconds
                heartbeat_timeout: 30000,
            }
        }
    }

    impl CacheConfig {
        pub fn recommended() -> Self {
            CacheConfig {
                memory_size: 100 * 1024, // 100Mb of in-memory caching
                max_age: 60 * 60 * 1000, // 1 hour before the cache must be invalidated
                max_entry_size: 10 * 1024,
            }
        }
    }
}
