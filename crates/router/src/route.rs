//! The [`Route`] trait and the [`routes!`](crate::routes) macro.

use std::fmt::Debug;

use crate::path::Location;

/// A typed route. Implement with [`routes!`](crate::routes).
pub trait Route: Sized + Clone + PartialEq + Debug + 'static {
    /// Patterns in declaration order (first match wins).
    fn patterns() -> &'static [&'static str];
    fn parse_location(loc: &Location) -> Option<Self>;
    fn to_location(&self) -> Location;

    fn parse(path: &str) -> Option<Self> {
        Location::parse(path).and_then(|l| Self::parse_location(&l))
    }

    fn to_path(&self) -> String {
        self.to_location().format()
    }

    /// Problems with the declared patterns (param without a field, duplicates, misplaced
    /// `*rest`). Call `assert!(R::check().is_empty())` in a test; the router also
    /// `debug_assert`s it on creation.
    fn check() -> Vec<String>;

    /// Whether this value survives `parse(to_path())`. In-app navigation works regardless;
    /// a route that does not round-trip cannot be deep-linked or restored from a saved
    /// history (e.g. an empty path param).
    fn round_trips(&self) -> bool {
        Self::parse(&self.to_path()).as_ref() == Some(self)
    }
}

/// Declare a typed route enum with path patterns.
///
/// Fields bound by the pattern (`:id`, or `*rest` for the remainder) come from the path and
/// are strict (empty or unparsable values don't match). Other fields are **search params**
/// (query string): lenient, defaulting when absent or unparsable, and omitted when equal to
/// their default. Field types need `FromStr + Display + Default + PartialEq`.
/// Patterns are tried in order: put specific routes before general ones.
/// `*rest` is normalised: empty segments are dropped (`a//b/` becomes `a/b`).
/// Unknown query keys are ignored and not preserved.
///
/// ```
/// use guiraffe_router::{Route, routes};
/// routes! {
///     #[derive(Debug, Clone, PartialEq)]
///     pub enum AppRoute {
///         Home = "/",
///         Project { id: u64, tab: String } = "/projects/:id",
///         Docs { rest: String } = "/docs/*rest",
///     }
/// }
/// let r = AppRoute::parse("/projects/7?tab=files").unwrap();
/// assert_eq!(r, AppRoute::Project { id: 7, tab: "files".into() });
/// assert_eq!(r.to_path(), "/projects/7?tab=files");
/// assert_eq!(AppRoute::Project { id: 7, tab: String::new() }.to_path(), "/projects/7");
/// ```
#[macro_export]
macro_rules! routes {
    (
        $(#[$m:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vm:meta])* $var:ident $({ $($f:ident : $t:ty),* $(,)? })? = $pat:literal ),* $(,)?
        }
    ) => {
        $(#[$m])*
        $vis enum $name {
            $( $(#[$vm])* $var $({ $($f : $t),* })? ),*
        }

        impl $crate::Route for $name {
            fn patterns() -> &'static [&'static str] {
                &[$($pat),*]
            }

            fn check() -> ::std::vec::Vec<::std::string::String> {
                let mut problems = ::std::vec::Vec::new();
                $(
                    problems.extend($crate::matcher::check(stringify!($var), $pat, &[$($(stringify!($f)),*)?]));
                )*
                problems
            }

            #[allow(unused_variables)]
            fn parse_location(loc: &$crate::Location) -> ::core::option::Option<Self> {
                $(
                    if let ::core::option::Option::Some(caps) = $crate::matcher::match_pattern($pat, &loc.segments) {
                        #[allow(clippy::redundant_closure_call)]
                        let built: ::core::option::Option<Self> = (|| {
                            ::core::option::Option::Some($name::$var {
                                $($( $f: $crate::matcher::field::<$t>(stringify!($f), &caps, loc)? ),*)?
                            })
                        })();
                        if built.is_some() {
                            return built;
                        }
                    }
                )*
                ::core::option::Option::None
            }

            fn to_location(&self) -> $crate::Location {
                match self {
                    $(
                        $name::$var { $($($f),*)? } => {
                            #[allow(unused_mut)]
                            let mut values: ::std::vec::Vec<(&'static str, ::std::string::String)> = ::std::vec::Vec::new();
                            #[allow(unused_mut)]
                            let mut defaults: ::std::vec::Vec<bool> = ::std::vec::Vec::new();
                            $($(
                                values.push((stringify!($f), $crate::matcher::display($f)));
                                defaults.push($crate::matcher::is_default($f));
                            )*)?
                            $crate::matcher::build($pat, values, &defaults)
                        }
                    )*
                }
            }
        }
    };
}
