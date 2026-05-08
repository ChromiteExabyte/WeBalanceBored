//! Calibration: convert raw 16-bit sensor values into kilograms per corner,
//! then into a normalized center-of-gravity reading.
//!
//! The Balance Board ships with three reference points stored in EEPROM at
//! extension registers `0xa40024..=0xa4003b` (24 bytes): the raw value each
//! sensor produces under known loads of 0 kg, 17 kg, and 34 kg. We linearly
//! interpolate between these to recover physical units.
//!
//! # References
//!
//! - WiiBrew Wii Balance Board — Calibration Data:
//!   <https://wiibrew.org/wiki/Wii_Balance_Board#Calibration_Data>

use crate::cog::CenterOfGravity;
use crate::error::CalibrationError;
use crate::sensors::{RawSensors, SensorQuad};

/// Three calibration reference points stored in the board's EEPROM.
///
/// Layout in the 24-byte EEPROM block (offsets are relative to the start
/// of the block, not absolute register addresses):
///
/// | Offset | Length | Meaning                                |
/// |--------|--------|----------------------------------------|
/// | `0..8` | 8      | Raw values at 0 kg per sensor          |
/// | `8..16`| 8      | Raw values at 17 kg per sensor         |
/// | `16..24`| 8     | Raw values at 34 kg per sensor         |
///
/// Each 8-byte chunk holds four 16-bit big-endian values in report order
/// (top-right, bottom-right, top-left, bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    /// Raw values reported when 0 kg is on each sensor.
    pub kg0: SensorQuad<u16>,
    /// Raw values reported when 17 kg is on each sensor.
    pub kg17: SensorQuad<u16>,
    /// Raw values reported when 34 kg is on each sensor.
    pub kg34: SensorQuad<u16>,
}

impl Calibration {
    /// Parse the 24-byte EEPROM calibration block.
    ///
    /// Validates that for every sensor `kg0 < kg17 < kg34`. If that fails
    /// (typically: uninitialized or corrupted EEPROM), returns
    /// [`CalibrationError::Nonmonotonic`] rather than producing a calibration
    /// that would give nonsensical or infinite weights.
    pub fn from_eeprom(bytes: &[u8; 24]) -> Result<Self, CalibrationError> {
        let kg0 = read_quad(&bytes[0..8]);
        let kg17 = read_quad(&bytes[8..16]);
        let kg34 = read_quad(&bytes[16..24]);

        let monotonic = |a: u16, b: u16, c: u16| a < b && b < c;
        if !(monotonic(kg0.top_right, kg17.top_right, kg34.top_right)
            && monotonic(kg0.bottom_right, kg17.bottom_right, kg34.bottom_right)
            && monotonic(kg0.top_left, kg17.top_left, kg34.top_left)
            && monotonic(kg0.bottom_left, kg17.bottom_left, kg34.bottom_left))
        {
            return Err(CalibrationError::Nonmonotonic);
        }
        Ok(Calibration { kg0, kg17, kg34 })
    }

    /// Convert raw sensor readings to kilograms per sensor using piecewise
    /// linear interpolation between the three calibration points.
    ///
    /// Values at or below `kg0` are clamped to 0 (the board's unloaded
    /// reading drifts slightly with temperature). Values above `kg34`
    /// extrapolate linearly using the upper segment's slope.
    #[must_use]
    pub fn calibrate(&self, raw: RawSensors) -> CalibratedSensors {
        CalibratedSensors {
            top_right: interpolate(
                raw.top_right,
                self.kg0.top_right,
                self.kg17.top_right,
                self.kg34.top_right,
            ),
            bottom_right: interpolate(
                raw.bottom_right,
                self.kg0.bottom_right,
                self.kg17.bottom_right,
                self.kg34.bottom_right,
            ),
            top_left: interpolate(
                raw.top_left,
                self.kg0.top_left,
                self.kg17.top_left,
                self.kg34.top_left,
            ),
            bottom_left: interpolate(
                raw.bottom_left,
                self.kg0.bottom_left,
                self.kg17.bottom_left,
                self.kg34.bottom_left,
            ),
        }
    }
}

/// Calibrated sensor readings, in kilograms per corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibratedSensors {
    /// Top-right corner load in kg.
    pub top_right: f32,
    /// Bottom-right corner load in kg.
    pub bottom_right: f32,
    /// Top-left corner load in kg.
    pub top_left: f32,
    /// Bottom-left corner load in kg.
    pub bottom_left: f32,
}

impl CalibratedSensors {
    /// Total weight on the board, summed across all four sensors, in kg.
    #[must_use]
    pub fn total_kg(&self) -> f32 {
        self.top_right + self.bottom_right + self.top_left + self.bottom_left
    }

    /// Compute the center of gravity, normalized to `[-1.0, +1.0]` per axis.
    ///
    /// Returns `None` when total weight is below `min_total_kg`, since COG
    /// is undefined for an unloaded board. A typical threshold is 2 kg —
    /// enough to filter sensor noise without rejecting a small child.
    #[must_use]
    pub fn center_of_gravity(&self, min_total_kg: f32) -> Option<CenterOfGravity> {
        let total = self.total_kg();
        if total < min_total_kg || total <= 0.0 {
            return None;
        }
        let right = self.top_right + self.bottom_right;
        let left = self.top_left + self.bottom_left;
        let top = self.top_right + self.top_left;
        let bottom = self.bottom_right + self.bottom_left;
        Some(CenterOfGravity {
            x: (right - left) / total,
            y: (top - bottom) / total,
        })
    }
}

fn read_quad(chunk: &[u8]) -> SensorQuad<u16> {
    SensorQuad {
        top_right: u16::from_be_bytes([chunk[0], chunk[1]]),
        bottom_right: u16::from_be_bytes([chunk[2], chunk[3]]),
        top_left: u16::from_be_bytes([chunk[4], chunk[5]]),
        bottom_left: u16::from_be_bytes([chunk[6], chunk[7]]),
    }
}

fn interpolate(raw: u16, c0: u16, c17: u16, c34: u16) -> f32 {
    let raw = f32::from(raw);
    let c0 = f32::from(c0);
    let c17 = f32::from(c17);
    let c34 = f32::from(c34);
    if raw <= c0 {
        0.0
    } else if raw < c17 {
        17.0 * (raw - c0) / (c17 - c0)
    } else {
        17.0 + 17.0 * (raw - c17) / (c34 - c17)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uniform_calibration() -> Calibration {
        // Every sensor: 5000 raw at 0kg, 10000 at 17kg, 15000 at 34kg.
        // 5000 raw counts per 17kg => linear, easy to reason about in tests.
        let pt = |v| SensorQuad {
            top_right: v,
            bottom_right: v,
            top_left: v,
            bottom_left: v,
        };
        Calibration {
            kg0: pt(5000),
            kg17: pt(10000),
            kg34: pt(15000),
        }
    }

    #[test]
    fn calibrates_zero_load() {
        let cal = uniform_calibration();
        let raw = SensorQuad {
            top_right: 5000,
            bottom_right: 5000,
            top_left: 5000,
            bottom_left: 5000,
        };
        assert_eq!(cal.calibrate(raw).total_kg(), 0.0);
    }

    #[test]
    fn calibrates_17kg_per_sensor_to_68kg_total() {
        let cal = uniform_calibration();
        let raw = SensorQuad {
            top_right: 10000,
            bottom_right: 10000,
            top_left: 10000,
            bottom_left: 10000,
        };
        let kg = cal.calibrate(raw);
        assert!((kg.total_kg() - 68.0).abs() < 0.001);
    }

    #[test]
    fn calibrates_lower_segment_midpoint() {
        let cal = uniform_calibration();
        // 7500 is halfway between kg0 (5000) and kg17 (10000) -> 8.5 kg per sensor -> 34 kg total.
        let raw = SensorQuad {
            top_right: 7500,
            bottom_right: 7500,
            top_left: 7500,
            bottom_left: 7500,
        };
        assert!((cal.calibrate(raw).total_kg() - 34.0).abs() < 0.001);
    }

    #[test]
    fn calibrates_upper_segment_midpoint() {
        let cal = uniform_calibration();
        // 12500 is halfway between kg17 (10000) and kg34 (15000) -> 25.5 kg per sensor -> 102 kg total.
        let raw = SensorQuad {
            top_right: 12500,
            bottom_right: 12500,
            top_left: 12500,
            bottom_left: 12500,
        };
        assert!((cal.calibrate(raw).total_kg() - 102.0).abs() < 0.001);
    }

    #[test]
    fn clamps_below_zero_load() {
        let cal = uniform_calibration();
        let raw = SensorQuad {
            top_right: 0,
            bottom_right: 0,
            top_left: 0,
            bottom_left: 0,
        };
        assert_eq!(cal.calibrate(raw).total_kg(), 0.0);
    }

    #[test]
    fn extrapolates_above_34kg() {
        let cal = uniform_calibration();
        // 20000 = 5000 above kg34 (15000); upper slope is 17kg/5000raw, so +17kg.
        let raw = SensorQuad {
            top_right: 20000,
            bottom_right: 20000,
            top_left: 20000,
            bottom_left: 20000,
        };
        let kg = cal.calibrate(raw);
        assert!((kg.total_kg() - (4.0 * 51.0)).abs() < 0.001);
    }

    #[test]
    fn cog_centered_when_balanced() {
        let kg = CalibratedSensors {
            top_right: 20.0,
            bottom_right: 20.0,
            top_left: 20.0,
            bottom_left: 20.0,
        };
        let cog = kg.center_of_gravity(1.0).unwrap();
        assert!(cog.x.abs() < 0.001);
        assert!(cog.y.abs() < 0.001);
    }

    #[test]
    fn cog_full_right_when_only_right_sensors_loaded() {
        let kg = CalibratedSensors {
            top_right: 20.0,
            bottom_right: 20.0,
            top_left: 0.0,
            bottom_left: 0.0,
        };
        let cog = kg.center_of_gravity(1.0).unwrap();
        assert!((cog.x - 1.0).abs() < 0.001);
        assert!(cog.y.abs() < 0.001);
    }

    #[test]
    fn cog_full_forward_when_only_top_sensors_loaded() {
        let kg = CalibratedSensors {
            top_right: 20.0,
            bottom_right: 0.0,
            top_left: 20.0,
            bottom_left: 0.0,
        };
        let cog = kg.center_of_gravity(1.0).unwrap();
        assert!(cog.x.abs() < 0.001);
        assert!((cog.y - 1.0).abs() < 0.001);
    }

    #[test]
    fn cog_undefined_below_min_weight() {
        let kg = CalibratedSensors {
            top_right: 0.1,
            bottom_right: 0.1,
            top_left: 0.1,
            bottom_left: 0.1,
        };
        assert!(kg.center_of_gravity(1.0).is_none());
    }

    #[test]
    fn rejects_zeroed_eeprom() {
        let bad = [0u8; 24];
        assert_eq!(
            Calibration::from_eeprom(&bad),
            Err(CalibrationError::Nonmonotonic)
        );
    }

    #[test]
    fn parses_eeprom_layout_in_report_order() {
        // 0kg point: TR=0x0100, BR=0x0200, TL=0x0300, BL=0x0400
        // 17kg: each +0x1000.  34kg: each +0x2000.
        let bytes = [
            0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x11, 0x00, 0x12, 0x00, 0x13, 0x00,
            0x14, 0x00, 0x21, 0x00, 0x22, 0x00, 0x23, 0x00, 0x24, 0x00,
        ];
        let cal = Calibration::from_eeprom(&bytes).unwrap();
        assert_eq!(cal.kg0.top_right, 0x0100);
        assert_eq!(cal.kg0.bottom_left, 0x0400);
        assert_eq!(cal.kg17.bottom_right, 0x1200);
        assert_eq!(cal.kg34.top_left, 0x2300);
    }
}
