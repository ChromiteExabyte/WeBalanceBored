//! Center-of-gravity output type.

/// Center of gravity, in normalized board coordinates.
///
/// Both axes range over `[-1.0, +1.0]`, with `(0, 0)` at the geometric
/// center of the board. Sign conventions match a typical right-handed
/// joystick:
///
/// - `x`: `-1.0` = full left, `+1.0` = full right.
/// - `y`: `-1.0` = full back (player side), `+1.0` = full forward (TV side).
///
/// "Forward" here is the edge of the board farther from the player when
/// they stand on it facing the TV — the same edge as the "top" sensors.
/// For Superflight this maps naturally: leaning forward = nose down = dive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CenterOfGravity {
    /// Left/right axis. `-1.0` = full left, `+1.0` = full right, `0` = centered.
    pub x: f32,
    /// Front/back axis. `-1.0` = full back, `+1.0` = full forward, `0` = centered.
    pub y: f32,
}

impl CenterOfGravity {
    /// Distance from center, useful for radial deadzones / thresholds.
    #[must_use]
    pub fn magnitude(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_at_center_is_zero() {
        let c = CenterOfGravity { x: 0.0, y: 0.0 };
        assert_eq!(c.magnitude(), 0.0);
    }

    #[test]
    fn magnitude_at_full_right_is_one() {
        let c = CenterOfGravity { x: 1.0, y: 0.0 };
        assert!((c.magnitude() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn magnitude_at_corner_is_sqrt2() {
        let c = CenterOfGravity { x: 1.0, y: 1.0 };
        assert!((c.magnitude() - 2f32.sqrt()).abs() < 1e-6);
    }
}
