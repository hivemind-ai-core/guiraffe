//! Size tokens, owned by `guiraffe-core`.
//!
//! The layout crate uses the core scale directly: this module only re-exports
//! it, so a widget can say `Size::Md` without reaching into another crate.

pub use guiraffe_core::Size;
