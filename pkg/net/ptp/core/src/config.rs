use ptp_proto::ptp::TimeSyncConfig;


const DEFAULT_CONFIG_PROTO: &'static str = r#"
    leader {
        sync_interval_seconds: 0.25
        followers: []
        ptp_timeout_seconds: 0.05
        sync_timeout_seconds: 0.1
        realtime_clock_sync {
            p_scale: 0 # 0.01
            i_scale: 0 # 0.0001
            max_offset_seconds: 1
            max_error_integral_secs: 0.05
            max_correction_ppm: 50
        }
    }

    follower {
        leader_clock_sync {
            p_scale: 0.5
            i_scale: 0.05

            # The set_offset feature of the BCM chip is not good and only gets us within 5ms of the right time.
            max_offset_seconds: 0.01 # 0.0001 # 100us
            max_error_integral_secs: 0.05

            # Must be larger than the leader -> ntp one
            max_correction_ppm: 100
        }
        max_network_rtt: 0.00001 # 10us
        max_sample_age: 0.1
    }

"#;

lazy_static! {
    static ref DEFAULT_CONFIG: TimeSyncConfig = {
        let mut ptp_config = TimeSyncConfig::default();
        protobuf::text::parse_text_proto(DEFAULT_CONFIG_PROTO, &mut ptp_config).unwrap();
        ptp_config
    };
}


pub fn default_config() -> TimeSyncConfig {
    DEFAULT_CONFIG.clone()
}