use balance_board_protocol::CalibratedSensors;
use std::time::{Duration, Instant};

/// Session-only weight baseline, sampled after the front button is released.
#[derive(Default)]
pub(crate) struct WeightZero {
    offset: [f32; 4],
    sum: [f32; 4],
    count: usize,
    released: Option<Instant>,
    pending: bool,
    held: bool,
}

impl WeightZero {
    pub fn pending(&self) -> bool {
        self.pending
    }

    /// Returns corrected weights and whether a new baseline was completed.
    pub fn observe(
        &mut self,
        weights: CalibratedSensors,
        button: bool,
        now: Instant,
    ) -> (CalibratedSensors, bool) {
        let values = [
            weights.top_right,
            weights.bottom_right,
            weights.top_left,
            weights.bottom_left,
        ];
        if button && !self.held {
            self.pending = true;
            self.released = None;
            self.sum = [0.0; 4];
            self.count = 0;
        }
        if !button && self.held && self.pending {
            self.released = Some(now);
        }
        self.held = button;
        let mut completed = false;
        if self.pending && !button {
            if let Some(released) = self.released {
                let elapsed = now.duration_since(released);
                // Let the pressure from pressing the button settle first.
                if elapsed >= Duration::from_millis(500) {
                    for (sum, value) in self.sum.iter_mut().zip(values) {
                        *sum += value;
                    }
                    self.count += 1;
                    if elapsed >= Duration::from_millis(1500) && self.count >= 2 {
                        self.offset = self.sum.map(|v| v / self.count as f32);
                        self.pending = false;
                        completed = true;
                    }
                }
            }
        }
        let corrected: [f32; 4] = std::array::from_fn(|i| (values[i] - self.offset[i]).max(0.0));
        (
            CalibratedSensors {
                top_right: corrected[0],
                bottom_right: corrected[1],
                top_left: corrected[2],
                bottom_left: corrected[3],
            },
            completed,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kg(v: f32) -> CalibratedSensors {
        CalibratedSensors {
            top_right: v,
            bottom_right: v,
            top_left: v,
            bottom_left: v,
        }
    }

    #[test]
    fn waits_for_release_and_settling_then_subtracts_baseline_from_future_load() {
        let mut zero = WeightZero::default();
        let start = Instant::now();
        zero.observe(kg(5.0), true, start);
        zero.observe(kg(5.0), true, start + Duration::from_secs(2));
        assert!(zero.pending());
        zero.observe(kg(5.0), false, start + Duration::from_secs(3));
        zero.observe(kg(0.25), false, start + Duration::from_millis(3500));
        let (reading, done) = zero.observe(kg(0.25), false, start + Duration::from_millis(4500));
        assert!(done);
        assert_eq!(reading.total_kg(), 0.0);
        assert!(!zero.pending());
        let (loaded, done) = zero.observe(kg(10.25), false, start + Duration::from_secs(5));
        assert!(!done);
        assert_eq!(loaded.total_kg(), 40.0);
        assert_eq!(
            zero.observe(kg(0.1), false, start + Duration::from_secs(6))
                .0
                .total_kg(),
            0.0
        );
    }

    #[test]
    fn repeated_press_replaces_baseline_using_original_weights() {
        let mut zero = WeightZero::default();
        let start = Instant::now();
        for (seconds, baseline) in [(0, 0.25), (4, 0.5)] {
            let now = start + Duration::from_secs(seconds);
            zero.observe(kg(5.0), true, now);
            zero.observe(kg(baseline), false, now + Duration::from_millis(100));
            zero.observe(kg(baseline), false, now + Duration::from_millis(600));
            assert!(
                zero.observe(kg(baseline), false, now + Duration::from_millis(1600))
                    .1
            );
        }
        assert_eq!(
            zero.observe(kg(10.5), false, start + Duration::from_secs(7))
                .0
                .total_kg(),
            40.0
        );
    }
}
