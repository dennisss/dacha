use ptp_proto::ptp::TimeSyncConfig;


// NOTE: the max PPM adjustment allowed by linux is +/- 500ppm
// TODO: Do a much better job of tuning all these parameters.
const DEFAULT_CONFIG_TEMPLATE_PROTO: &'static str = r#"
    leader {
        sync_interval_seconds: 0.25
        followers: []
        ptp_timeout_seconds: 0.05
        sync_timeout_seconds: 0.1
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

    basic_client {
        server_addr: ""
        sync_interval_seconds: 0.5

        clock_sync {
            p_scale: 0.5
            i_scale: 0.05
            max_offset_seconds: 1
            max_error_integral_secs: 0.01
            max_correction_ppm: 50 # Must be > crystal ppm
            max_frequency_step_ppm: 0.001
        }

        max_network_rtt: 0.005 # 5ms
    }
    
    realtime_clock_sync {
        p_scale: 0 # 0.01
        i_scale: 0 # 0.0001
        max_offset_seconds: 1
        max_error_integral_secs: 10
        max_correction_ppm: 50
    }

"#;

lazy_static! {
    static ref DEFAULT_CONFIG_TEMPLATE: TimeSyncConfig = {
        let mut ptp_config = TimeSyncConfig::default();
        protobuf::text::parse_text_proto(DEFAULT_CONFIG_TEMPLATE_PROTO, &mut ptp_config).unwrap();
        ptp_config
    };
}


pub fn default_config_template() -> TimeSyncConfig {
    DEFAULT_CONFIG_TEMPLATE.clone()
}