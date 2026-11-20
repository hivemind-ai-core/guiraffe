#[test]
fn invalid_ui_has_source_diagnostics() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
