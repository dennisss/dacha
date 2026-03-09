
pub struct RevolutionTracker {
    last_angle: f64,
}

impl RevolutionTracker {
    pub fn new(initial_angle: f64) -> Self {
        Self {
            last_angle: initial_angle
        }
    }

    pub fn next(&mut self, angle: f64) -> f64 {
        assert!(angle >= 0.0 && angle < 1.0);

        let rev = self.last_angle.floor();

        let options = [
            rev + angle,
            rev + 1.0 + angle,
            rev - 1.0 + angle
        ];

        let mut best_option = 0;
        let mut best_distance = 100.0;

        for i in 0..options.len() {
            let dist = (options[i] - self.last_angle).abs();
            if dist < best_distance {
                best_distance = dist;
                best_option = i;
            }
        }

        let extended_angle = options[best_option];
        self.last_angle = extended_angle;
        extended_angle
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {

        let mut tracker = RevolutionTracker::new(0.0);

        assert_eq!(tracker.next(0.1), 0.1);
        assert_eq!(tracker.next(0.05), 0.05);
        assert_eq!(tracker.next(0.4), 0.4);
        assert_eq!(tracker.next(0.8), 0.8);
        assert_eq!(tracker.next(0.2), 1.2);
        assert_eq!(tracker.next(0.1), 1.1);
        assert_eq!(tracker.next(0.9), 0.9);
        assert_eq!(tracker.next(0.2), 1.2);
        assert_eq!(tracker.next(0.5), 1.5);
        assert_eq!(tracker.next(0.7), 1.7);
        assert_eq!(tracker.next(0.1), 2.1);
    }

}