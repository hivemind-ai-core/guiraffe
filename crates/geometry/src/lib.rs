//! GPUI-free geometry shared by GUIraffe crates.
//!
//! [`AxisLock`] is the optional lock any drag gesture can adopt: the default leaves a drag
//! free, [`AxisLock::LockX`] keeps x so the drag moves vertically only, and
//! [`AxisLock::LockY`] keeps y so it moves horizontally only. [`apply`] constrains a move;
//! [`AxisLock::dominant`] picks the lock for a temporary modifier (Shift in most editors).
#![forbid(unsafe_code)]

/// A point or vector in any coordinate space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Which coordinate a drag may not change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum AxisLock {
    /// The drag moves freely.
    #[default]
    Unlocked,
    /// x stays put: the drag moves vertically only.
    LockX,
    /// y stays put: the drag moves horizontally only.
    LockY,
}

impl AxisLock {
    /// The lock a "constrain to the dominant axis" modifier applies to a move from `from` to
    /// `to`: the axis the pointer moved less along is locked. Ties lock y (horizontal moves).
    pub fn dominant(from: Point, to: Point) -> AxisLock {
        if (to.y - from.y).abs() > (to.x - from.x).abs() {
            AxisLock::LockX
        } else {
            AxisLock::LockY
        }
    }

    /// This lock, or the dominant-axis lock for the move when unlocked and `modifier` is held.
    /// An explicit lock wins over the modifier.
    pub fn with_modifier(self, modifier: bool, from: Point, to: Point) -> AxisLock {
        match self {
            AxisLock::Unlocked if modifier => AxisLock::dominant(from, to),
            lock => lock,
        }
    }
}

/// Where a move from `from` to `to` ends under `lock`: the locked coordinate keeps its value
/// from `from`.
pub fn apply(lock: AxisLock, from: Point, to: Point) -> Point {
    match lock {
        AxisLock::Unlocked => to,
        AxisLock::LockX => Point::new(from.x, to.y),
        AxisLock::LockY => Point::new(to.x, from.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FROM: Point = Point::new(10., 20.);
    const TO: Point = Point::new(13., 60.);

    #[test]
    fn each_lock_keeps_its_coordinate() {
        assert_eq!(apply(AxisLock::Unlocked, FROM, TO), TO);
        assert_eq!(apply(AxisLock::LockX, FROM, TO), Point::new(10., 60.));
        assert_eq!(apply(AxisLock::LockY, FROM, TO), Point::new(13., 20.));
    }

    #[test]
    fn the_modifier_locks_the_minor_axis_unless_already_locked() {
        assert_eq!(
            AxisLock::Unlocked.with_modifier(true, FROM, TO),
            AxisLock::LockX
        );
        assert_eq!(
            AxisLock::Unlocked.with_modifier(true, FROM, Point::new(50., 22.)),
            AxisLock::LockY
        );
        assert_eq!(
            AxisLock::Unlocked.with_modifier(false, FROM, TO),
            AxisLock::Unlocked
        );
        assert_eq!(
            AxisLock::LockY.with_modifier(true, FROM, TO),
            AxisLock::LockY
        );
    }

    #[test]
    fn the_default_is_unlocked() {
        assert_eq!(AxisLock::default(), AxisLock::Unlocked);
    }
}
