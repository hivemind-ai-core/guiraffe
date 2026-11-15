//! Finite world geometry and a pointer-anchored camera.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
    pub fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    pub fn snap(self, grid: f32) -> Self {
        if !grid.is_finite() || grid <= 0. {
            return self;
        }
        Self::new(
            (self.x / grid).round() * grid,
            (self.y / grid).round() * grid,
        )
    }
}
impl std::ops::Add for Point {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y)
    }
}
impl std::ops::Sub for Point {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y)
    }
}
impl std::ops::Mul<f32> for Point {
    type Output = Self;
    fn mul(self, b: f32) -> Self {
        Self::new(self.x * b, self.y * b)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub origin: Point,
    pub size: Point,
}
impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Point::new(width, height),
        }
    }
    pub fn between(a: Point, b: Point) -> Self {
        Self::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        )
    }
    pub fn valid(self) -> bool {
        self.origin.finite()
            && self.size.finite()
            && self.size.x >= 0.
            && self.size.y >= 0.
            && (self.origin + self.size).finite()
    }
    pub fn max(self) -> Point {
        self.origin + self.size
    }
    pub fn center(self) -> Point {
        self.origin + self.size * 0.5
    }
    pub fn contains(self, p: Point) -> bool {
        self.intersects(Self {
            origin: p,
            size: Point::default(),
        })
    }
    pub fn intersects(self, b: Self) -> bool {
        self.valid()
            && b.valid()
            && self.origin.x <= b.max().x
            && self.max().x >= b.origin.x
            && self.origin.y <= b.max().y
            && self.max().y >= b.origin.y
    }
    pub fn union(self, b: Self) -> Self {
        Self::new(
            self.origin.x.min(b.origin.x),
            self.origin.y.min(b.origin.y),
            self.max().x.max(b.max().x) - self.origin.x.min(b.origin.x),
            self.max().y.max(b.max().y) - self.origin.y.min(b.origin.y),
        )
    }
    pub fn expand(self, amount: f32) -> Self {
        Self::new(
            self.origin.x - amount,
            self.origin.y - amount,
            self.size.x + 2. * amount,
            self.size.y + 2. * amount,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub pan: Point,
    pub zoom: f32,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan: Point::default(),
            zoom: 1.,
        }
    }
}
impl Viewport {
    pub const MIN_ZOOM: f32 = 0.02;
    pub const MAX_ZOOM: f32 = 4.;
    pub fn valid(self) -> bool {
        self.pan.finite()
            && self.zoom.is_finite()
            && (Self::MIN_ZOOM..=Self::MAX_ZOOM).contains(&self.zoom)
    }
    pub fn screen(self, world: Point) -> Point {
        world * self.zoom + self.pan
    }
    pub fn world(self, screen: Point) -> Point {
        (screen - self.pan) * (1. / self.zoom)
    }
    pub fn visible(self, size: Point) -> Rect {
        Rect {
            origin: self.world(Point::default()),
            size: size * (1. / self.zoom),
        }
    }
    pub fn zoom_at(&mut self, at: Point, factor: f32) {
        if !at.finite() || !factor.is_finite() || factor <= 0. || !self.valid() {
            return;
        }
        let world = self.world(at);
        self.zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.pan = at - world * self.zoom;
    }
    pub fn fit(&mut self, bounds: Rect, size: Point, padding: f32) {
        if !bounds.valid()
            || !size.finite()
            || !padding.is_finite()
            || padding < 0.
            || size.x <= padding * 2.
            || size.y <= padding * 2.
        {
            return;
        }
        self.zoom = ((size.x - 2. * padding) / bounds.size.x.max(1.))
            .min((size.y - 2. * padding) / bounds.size.y.max(1.))
            .clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.pan = size * 0.5 - bounds.center() * self.zoom;
    }
}
