//! Pure sRGB color math (no GPUI), so palette/variant logic is testable headlessly.

/// An opaque 8-bit sRGB color with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgb {
    pub const fn hex(hex: u32) -> Self {
        Self {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
            a: 255,
        }
    }

    pub const WHITE: Rgb = Rgb::hex(0xffffff);
    pub const BLACK: Rgb = Rgb::hex(0x000000);
    pub const TRANSPARENT: Rgb = Rgb {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// Same color with alpha `alpha` (clamped to 0..=1).
    pub fn alpha(self, alpha: f32) -> Self {
        Self {
            a: (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
            ..self
        }
    }

    pub fn to_u32(self) -> u32 {
        (u32::from(self.r) << 16) | (u32::from(self.g) << 8) | u32::from(self.b)
    }

    /// Linear interpolation towards `other` by `t` in 0..=1 (alpha included).
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let l = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Rgb {
            r: l(self.r, other.r),
            g: l(self.g, other.g),
            b: l(self.b, other.b),
            a: l(self.a, other.a),
        }
    }

    /// Composite this (possibly translucent) color over an opaque `bg`.
    pub fn over(self, bg: Rgb) -> Rgb {
        let opaque = Rgb { a: 255, ..self };
        Rgb {
            a: 255,
            ..bg.mix(opaque, f32::from(self.a) / 255.0)
        }
    }

    /// WCAG relative luminance (ignores alpha).
    pub fn luminance(self) -> f32 {
        let ch = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(self.r) + 0.7152 * ch(self.g) + 0.0722 * ch(self.b)
    }

    /// WCAG contrast ratio, 1.0..=21.0.
    pub fn contrast(self, other: Rgb) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Black or white text for this background, using Mantine's `autoContrast`
    /// luminance threshold (0.3) so primary blues keep white text like Mantine.
    pub fn readable_foreground(self) -> Rgb {
        if self.luminance() > 0.3 {
            Rgb::BLACK
        } else {
            Rgb::WHITE
        }
    }

    pub fn to_hsla(self) -> gpui_kit::Hsla {
        let mut rgba = gpui_kit::rgb(self.to_u32());
        rgba.a = f32::from(self.a) / 255.0;
        rgba.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(Rgb::hex(0x228be6).to_u32(), 0x228be6);
        assert_eq!(
            Rgb::hex(0x228be6),
            Rgb {
                r: 0x22,
                g: 0x8b,
                b: 0xe6,
                a: 255
            }
        );
    }

    #[test]
    fn contrast_extremes() {
        assert!((Rgb::WHITE.contrast(Rgb::BLACK) - 21.0).abs() < 0.01);
        assert!((Rgb::WHITE.contrast(Rgb::WHITE) - 1.0).abs() < 1e-6);
        assert_eq!(Rgb::hex(0xfff3bf).readable_foreground(), Rgb::BLACK);
        assert_eq!(Rgb::hex(0x1971c2).readable_foreground(), Rgb::WHITE);
    }

    #[test]
    fn mix_and_over() {
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 0.0), Rgb::BLACK);
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 1.0), Rgb::WHITE);
        assert_eq!(Rgb::BLACK.mix(Rgb::WHITE, 2.0), Rgb::WHITE, "t clamps");
        assert_eq!(Rgb::WHITE.alpha(0.0).over(Rgb::BLACK), Rgb::BLACK);
        assert_eq!(Rgb::WHITE.alpha(1.0).over(Rgb::BLACK), Rgb::WHITE);
        assert_eq!(Rgb::WHITE.alpha(0.5).over(Rgb::BLACK).r, 128);
    }

    #[test]
    fn hsla_conversion_keeps_alpha() {
        let h = Rgb::hex(0xff0000).alpha(0.5).to_hsla();
        assert!((h.a - 0.5).abs() < 0.01);
        assert!(h.h.abs() < 1e-3 && (h.s - 1.0).abs() < 1e-3);
    }
}
