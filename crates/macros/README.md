
`ui!` expands named properties to ordinary Rust builder method calls. Use a
method expression such as `.section(42.0, ColorName::Blue)` for a builder with
multiple arguments or a repeated method. Constructors receive their regular
arguments, for example `ui! { PasswordStrength("A7!example") }`.
