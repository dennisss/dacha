use std::time::{Instant, Duration};
use std::collections::{HashMap, VecDeque};

use common::errors::*;

use crate::stats::MinMaxStats;

/// Maximum number of past 
///
/// TODO: Also need to kick out any points that are too old to represent the current skew
const HISTORY_SIZE: usize = 100;

const MAX_ERROR: u64 = 16_000_000 / 2; // 500ms

/// TODO: Ideally detect if there are large gaps between points in our data and block and conversions in those dead zones as well (there must be at least 1 point before and 1 point after the queried time which are close though).
const MAX_INTERPOLATION_GAP: u64 = 10 * 16_000_000; // 10 seconds.



#[derive(Default)]
pub struct TimeRelation {
    // TODO: Need to monitor for staleness of this data and don't use extremely old values.
    
    /// NOTE: All the times in this should be monotonically increasing for each device.
    ///
    /// TODO: Store in the background thread so that we 
    points: VecDeque<TimeRelationPoint>,

    sample_count: usize,
}

impl TimeRelation {

    pub fn add_point(&mut self, point: TimeRelationPoint) {
        // TODO: Maybe add a filter to not accept points too close to the previous point.
        // We ideally want 

        // TODO: Validate that times are monotonic.

        self.points.push_back(point);
        self.sample_count += 1;

        while self.points.len() > HISTORY_SIZE {
            self.points.pop_front();
        }
    } 

    pub fn is_healthy(&self) -> bool {
        // TODO: Also factor in staleness.

        // TODO: Verify that the linear regression has low error.

        self.sample_count >= 4

    }

    pub fn total_seen_points(&self) -> usize {
        self.sample_count
    }

    pub fn convert_time(&self, time: u64, forward: bool) -> Result<u64> {
        if self.points.len() < 4 {
            return Err(err_msg("Not enough time samples collected yet to do time conversions."));
        }

        let (min_window, max_window) = {
            let first = &self.points[0];
            let last = self.points.back().unwrap();
            
            if forward {
                (first.time1, last.time1) 
            } else {
                (first.time2, last.time2)
            }
        };

        let distance = core::cmp::min(
            ((time as i64) - (min_window as i64)).abs(),
            ((time as i64) - (max_window as i64)).abs()
        ) as u64;

        if distance > MAX_INTERPOLATION_GAP {
            return Err(err_msg("Time too far in the future/past or device sync too stale"));
        }


        let (model, error) = LinearTimeRelation::estimate(&self.points);

        if error > MAX_ERROR {
            return Err(err_msg("Clock sync error too high"));
        }

        let other_time = {
            if forward {
                model.forward_compute(time)
            } else {
                model.reverse_compute(time)
            }
        };

        if other_time < 0 {
            // Probably a time before the boot time of the MCU.
            return Err(err_msg("Estimated a negative time"));
        }

        Ok(other_time as u64)
    }

    pub fn stats(&self) -> TimeRelationStats {
        let (model, error) = LinearTimeRelation::estimate(&self.points);

        let mut rtt = MinMaxStats::default();
        for point in &self.points {
            if let Some(r) = point.rtt {
                rtt.add(r);
            }
        }

        TimeRelationStats {
            max_error: error,
            skew: model.scale,
            rtt,
        }
    }
}

pub struct TimeRelationStats {
    pub max_error: u64,
    pub skew: f64,
    pub rtt: MinMaxStats<Duration>,
}


#[derive(Clone, Debug)]
pub struct TimeRelationPoint {
    pub time1: u64,
    pub time2: u64,

    // Extra metadata for the point

    pub frame_counter: Option<u64>,

    /// Should only be present for RTT based measurements
    pub rtt: Option<Duration>,
}

/// Linear regression based model to relate two clocks.
///
/// (time1 - offset1) * scale + bias = (time2 - offset2) 
struct LinearTimeRelation {
    offset1: i64,
    offset2: i64,
    scale: f64,
}

impl LinearTimeRelation {
    /*
    TODO: Need to validate that the relation returned by this looks reasonable:
    - Error should be low.
    - scale should be within 50ppm
    */
    fn estimate(points: &VecDeque<TimeRelationPoint>) -> (Self, u64) {
        let mut offset1 = points[0].time1 as i64;
        let mut offset2 = points[0].time2 as i64;

        // The offset between the time1 and time2 curves are calculated differently
        // depending on how the data was collected:
        // - For SOF data, we assume there is 'zero offset' from the data and we
        //   just use the above code to pick the first point as the offset.
        // - For USB round trips with the host, error is not gaussian so we pick the
        //   point with the smallest RTT and use that as the offset between the
        //   curves (this code).
        let mut min_rtt = Duration::from_secs(100);
        for i in 0..points.len() {
            if let Some(rtt) = points[i].rtt {
                if rtt < min_rtt {
                    min_rtt = rtt;
                    offset1 = points[i].time1 as i64;
                    offset2 = points[i].time2 as i64;
                }
            }
        }

        // Linear regression with one unknown
        // We shift all times to start at (0, 0) to improve numerical stability.
        let scale = {
            let mut sum_ab = 0.0;
            let mut sum_bb = 0.0;
            for i in 0..points.len() {
                let time1 = ((points[i].time1 as i64) - offset1) as f64;
                let time2 = ((points[i].time2 as i64) - offset2) as f64;

                sum_ab += time1*time2;
                sum_bb += time1*time1;
            }

            sum_ab / sum_bb
        };

        let inst = Self {
            offset1,
            offset2,
            scale,
        };

        let mut error = 0;
        for i in 0..points.len() {
            let computed_time2 = inst.forward_compute(points[i].time1);
            let e = (points[i].time2 as i64) - computed_time2;
            error = error.max(e.abs() as u64);
        }

        (inst, error)
    }

    /// Given 'time1', estimates 'time2'
    fn forward_compute(&self, time: u64) -> i64 {
        let time1 = ((time as i64) - self.offset1) as f64;
        let time2 = time1 * self.scale;
        (time2 as i64) + (self.offset2 as i64)
    }

    /// Given 'time2', estimates 'time1'
    fn reverse_compute(&self, time: u64) -> i64 {
        let time2 = ((time as i64) - self.offset2) as f64;
        let time1 = time2 / self.scale;
        (time1 as i64) + self.offset1 
    }
}
