//! Mantine's default 10-shade palettes.

use crate::color::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorName {
    Dark,
    Gray,
    Red,
    Pink,
    Grape,
    Violet,
    Indigo,
    #[default]
    Blue,
    Cyan,
    Teal,
    Green,
    Lime,
    Yellow,
    Orange,
}

/// A palette shade index, 0 (lightest) ..= 9 (darkest). Out-of-range values clamp.
pub type Shade = u8;

const fn p(x: [u32; 10]) -> [Rgb; 10] {
    let mut out = [Rgb::BLACK; 10];
    let mut i = 0;
    while i < 10 {
        out[i] = Rgb::hex(x[i]);
        i += 1;
    }
    out
}

const DARK: [Rgb; 10] = p([
    0xc9c9c9, 0xb8b8b8, 0x828282, 0x696969, 0x424242, 0x3b3b3b, 0x2e2e2e, 0x242424, 0x1f1f1f,
    0x141414,
]);
const GRAY: [Rgb; 10] = p([
    0xf8f9fa, 0xf1f3f5, 0xe9ecef, 0xdee2e6, 0xced4da, 0xadb5bd, 0x868e96, 0x495057, 0x343a40,
    0x212529,
]);
const RED: [Rgb; 10] = p([
    0xfff5f5, 0xffe3e3, 0xffc9c9, 0xffa8a8, 0xff8787, 0xff6b6b, 0xfa5252, 0xf03e3e, 0xe03131,
    0xc92a2a,
]);
const PINK: [Rgb; 10] = p([
    0xfff0f6, 0xffdeeb, 0xfcc2d7, 0xfaa2c1, 0xf783ac, 0xf06595, 0xe64980, 0xd6336c, 0xc2255c,
    0xa61e4d,
]);
const GRAPE: [Rgb; 10] = p([
    0xf8f0fc, 0xf3d9fa, 0xeebefa, 0xe599f7, 0xda77f2, 0xcc5de8, 0xbe4bdb, 0xae3ec9, 0x9c36b5,
    0x862e9c,
]);
const VIOLET: [Rgb; 10] = p([
    0xf3f0ff, 0xe5dbff, 0xd0bfff, 0xb197fc, 0x9775fa, 0x845ef7, 0x7950f2, 0x7048e8, 0x6741d9,
    0x5f3dc4,
]);
const INDIGO: [Rgb; 10] = p([
    0xedf2ff, 0xdbe4ff, 0xbac8ff, 0x91a7ff, 0x748ffc, 0x5c7cfa, 0x4c6ef5, 0x4263eb, 0x3b5bdb,
    0x364fc7,
]);
const BLUE: [Rgb; 10] = p([
    0xe7f5ff, 0xd0ebff, 0xa5d8ff, 0x74c0fc, 0x4dabf7, 0x339af0, 0x228be6, 0x1c7ed6, 0x1971c2,
    0x1864ab,
]);
const CYAN: [Rgb; 10] = p([
    0xe3fafc, 0xc5f6fa, 0x99e9f2, 0x66d9e8, 0x3bc9db, 0x22b8cf, 0x15aabf, 0x1098ad, 0x0c8599,
    0x0b7285,
]);
const TEAL: [Rgb; 10] = p([
    0xe6fcf5, 0xc3fae8, 0x96f2d7, 0x63e6be, 0x38d9a9, 0x20c997, 0x12b886, 0x0ca678, 0x099268,
    0x087f5b,
]);
const GREEN: [Rgb; 10] = p([
    0xebfbee, 0xd3f9d8, 0xb2f2bb, 0x8ce99a, 0x69db7c, 0x51cf66, 0x40c057, 0x37b24d, 0x2f9e44,
    0x2b8a3e,
]);
const LIME: [Rgb; 10] = p([
    0xf4fce3, 0xe9fac8, 0xd8f5a2, 0xc0eb75, 0xa9e34b, 0x94d82d, 0x82c91e, 0x74b816, 0x66a80f,
    0x5c940d,
]);
const YELLOW: [Rgb; 10] = p([
    0xfff9db, 0xfff3bf, 0xffec99, 0xffe066, 0xffd43b, 0xfcc419, 0xfab005, 0xf59f00, 0xf08c00,
    0xe67700,
]);
const ORANGE: [Rgb; 10] = p([
    0xfff4e6, 0xffe8cc, 0xffd8a8, 0xffc078, 0xffa94d, 0xff922b, 0xfd7e14, 0xf76707, 0xe8590c,
    0xd9480f,
]);

impl ColorName {
    pub const ALL: [ColorName; 14] = [
        Self::Dark,
        Self::Gray,
        Self::Red,
        Self::Pink,
        Self::Grape,
        Self::Violet,
        Self::Indigo,
        Self::Blue,
        Self::Cyan,
        Self::Teal,
        Self::Green,
        Self::Lime,
        Self::Yellow,
        Self::Orange,
    ];

    pub fn shades(self) -> &'static [Rgb; 10] {
        match self {
            Self::Dark => &DARK,
            Self::Gray => &GRAY,
            Self::Red => &RED,
            Self::Pink => &PINK,
            Self::Grape => &GRAPE,
            Self::Violet => &VIOLET,
            Self::Indigo => &INDIGO,
            Self::Blue => &BLUE,
            Self::Cyan => &CYAN,
            Self::Teal => &TEAL,
            Self::Green => &GREEN,
            Self::Lime => &LIME,
            Self::Yellow => &YELLOW,
            Self::Orange => &ORANGE,
        }
    }

    pub fn shade(self, shade: Shade) -> Rgb {
        self.shades()[usize::from(shade.min(9))]
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Gray => "gray",
            Self::Red => "red",
            Self::Pink => "pink",
            Self::Grape => "grape",
            Self::Violet => "violet",
            Self::Indigo => "indigo",
            Self::Blue => "blue",
            Self::Cyan => "cyan",
            Self::Teal => "teal",
            Self::Green => "green",
            Self::Lime => "lime",
            Self::Yellow => "yellow",
            Self::Orange => "orange",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shades_get_darker() {
        for c in ColorName::ALL {
            let s = c.shades();
            assert!(s[0].luminance() > s[9].luminance(), "{c:?}");
        }
    }

    #[test]
    fn shade_clamps_and_names_roundtrip() {
        assert_eq!(ColorName::Blue.shade(42), ColorName::Blue.shade(9));
        assert_eq!(ColorName::Blue.shade(6), Rgb::hex(0x228be6));
        for c in ColorName::ALL {
            assert_eq!(ColorName::from_name(c.name()), Some(c));
        }
        assert_eq!(ColorName::from_name("BLUE"), Some(ColorName::Blue));
        assert_eq!(ColorName::from_name("nope"), None);
    }
}
