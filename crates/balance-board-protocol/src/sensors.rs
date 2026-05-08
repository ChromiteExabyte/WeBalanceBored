//! The four-corner sensor layout shared by raw, calibrated, and calibration values.

/// Generic four-corner container.
///
/// The Balance Board has four load cells, one per corner. This type lets the
/// same layout serve raw `u16` readings, `f32` weights, and EEPROM calibration
/// constants without duplicating four-field structs everywhere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensorQuad<T> {
    /// Top-right corner. Orientation: looking down at the board with the
    /// power button on the right edge, this is the upper-right cell.
    pub top_right: T,
    /// Bottom-right corner.
    pub bottom_right: T,
    /// Top-left corner.
    pub top_left: T,
    /// Bottom-left corner.
    pub bottom_left: T,
}

impl<T> SensorQuad<T> {
    /// Construct a quad by calling `f` for each corner.
    pub fn from_fn<F: FnMut(Corner) -> T>(mut f: F) -> Self {
        Self {
            top_right: f(Corner::TopRight),
            bottom_right: f(Corner::BottomRight),
            top_left: f(Corner::TopLeft),
            bottom_left: f(Corner::BottomLeft),
        }
    }

    /// Map each value through `f`, preserving layout.
    pub fn map<U, F: FnMut(T) -> U>(self, mut f: F) -> SensorQuad<U> {
        SensorQuad {
            top_right: f(self.top_right),
            bottom_right: f(self.bottom_right),
            top_left: f(self.top_left),
            bottom_left: f(self.bottom_left),
        }
    }

    /// Borrow each value through `f` without consuming the quad.
    pub fn map_ref<U, F: FnMut(&T) -> U>(&self, mut f: F) -> SensorQuad<U> {
        SensorQuad {
            top_right: f(&self.top_right),
            bottom_right: f(&self.bottom_right),
            top_left: f(&self.top_left),
            bottom_left: f(&self.bottom_left),
        }
    }
}

/// Identifier for one of the four corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Corner {
    /// Top-right corner.
    TopRight,
    /// Bottom-right corner.
    BottomRight,
    /// Top-left corner.
    TopLeft,
    /// Bottom-left corner.
    BottomLeft,
}

/// Raw 16-bit sensor readings, straight from the wire, before calibration.
pub type RawSensors = SensorQuad<u16>;
