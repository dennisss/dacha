

/*
GPIOTE channel
PPI channel
Timer channel
*/

pub struct PulseTimerRequest {
    interval: u32,
    width: u32,
    start_time: u32,
    count: u32,
    high: bool,
}