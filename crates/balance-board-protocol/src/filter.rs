//! Temporal filtering for noisy sensor streams.
//!
//! A real human standing on a Balance Board generates a few-Hz wobble
//! that's invisible to them but feels like joystick jitter in a game.
//! This module provides a single-pole exponential moving average
//! (one-axis or two-axis) that takes the edge off without adding
//! noticeable lag at typical Wiimote 100 Hz report rates.

/// Two-axis exponential moving average filter.
///
/// `alpha` is the smoothing factor in `(0, 1]`:
///
/// - `alpha = 1.0` — no smoothing (output equals input).
/// - `alpha = 0.5` — output halves toward the new input each step.
/// - `alpha → 0`   — extremely smooth, very laggy.
///
/// At a 100 Hz update rate, `alpha = 0.4` reaches ~95% of a step input
/// in ~5 frames (~50 ms) — responsive enough for action games while
/// flattening typical body sway.
#[derive(Debug, Clone, Copy)]
pub struct LowPass2D {
    alpha: f32,
    state: Option<(f32, f32)>,
}

impl LowPass2D {
    /// Construct with the given smoothing factor. Panics if `alpha`
    /// isn't in `(0, 1]`.
    #[must_use]
    pub fn new(alpha: f32) -> Self {
        assert!(
            alpha > 0.0 && alpha <= 1.0,
            "LowPass2D alpha must be in (0, 1], got {alpha}"
        );
        Self { alpha, state: None }
    }

    /// Feed a new sample, return the smoothed output. The first sample
    /// passes through unmodified (no warm-up needed).
    pub fn update(&mut self, x: f32, y: f32) -> (f32, f32) {
        let next = match self.state {
            None => (x, y),
            Some((px, py)) => (px + (x - px) * self.alpha, py + (y - py) * self.alpha),
        };
        self.state = Some(next);
        next
    }

    /// Reset the filter so the next sample passes through unmodified.
    pub fn reset(&mut self) {
        self.state = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_passes_through() {
        let mut f = LowPass2D::new(0.4);
        assert_eq!(f.update(0.7, -0.3), (0.7, -0.3));
    }

    #[test]
    fn alpha_one_is_passthrough() {
        let mut f = LowPass2D::new(1.0);
        f.update(0.0, 0.0);
        assert_eq!(f.update(0.5, 0.5), (0.5, 0.5));
        assert_eq!(f.update(-1.0, 1.0), (-1.0, 1.0));
    }

    #[test]
    fn step_response_converges() {
        let mut f = LowPass2D::new(0.4);
        f.update(0.0, 0.0);
        // Feed a step input of 1.0; after many iterations we should
        // approach 1.0.
        let mut x = 0.0;
        for _ in 0..50 {
            (x, _) = f.update(1.0, 0.0);
        }
        assert!(x > 0.99, "expected x to converge near 1.0, got {x}");
    }

    #[test]
    fn step_response_attenuated_short_term() {
        let mut f = LowPass2D::new(0.4);
        f.update(0.0, 0.0);
        let (x, _) = f.update(1.0, 0.0);
        // alpha=0.4 means first step lands 40% of the way.
        assert!((x - 0.4).abs() < 1e-6, "expected 0.4 after 1 step, got {x}");
    }

    #[test]
    fn steady_input_stays_steady() {
        let mut f = LowPass2D::new(0.4);
        for _ in 0..10 {
            f.update(0.5, -0.5);
        }
        let (x, y) = f.update(0.5, -0.5);
        assert!((x - 0.5).abs() < 1e-3);
        assert!((y - (-0.5)).abs() < 1e-3);
    }

    #[test]
    fn reset_drops_history() {
        let mut f = LowPass2D::new(0.4);
        f.update(1.0, 1.0);
        f.update(1.0, 1.0);
        f.reset();
        assert_eq!(f.update(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    #[should_panic]
    fn rejects_alpha_zero() {
        let _ = LowPass2D::new(0.0);
    }

    #[test]
    #[should_panic]
    fn rejects_alpha_above_one() {
        let _ = LowPass2D::new(1.5);
    }
}
