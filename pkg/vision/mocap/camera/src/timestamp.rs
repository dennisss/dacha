use std::time::Duration;


const NANOS_PER_SECOND: u64 = 1_000_000_000;

pub fn rounded_frame_timestamp(time: Duration, frame_rate: u32) -> Duration {
    // Time between frames in nanoseconds.
    let frame_interval = NANOS_PER_SECOND / (frame_rate as u64);

    // Rounding the subsec_nanos portion of 'trigger_time_ptp' to a frame boundary.
    let nanos = 
        (((time.subsec_nanos() as f64) / (frame_interval as f64)).round() * (frame_interval as f64)) as u64;

    Duration::from_secs(time.as_secs()) + Duration::from_nanos(nanos)
}