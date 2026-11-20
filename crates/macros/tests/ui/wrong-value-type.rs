use guiraffe_macros::ui;

struct Button;
impl Button {
    fn new(_: &str) -> Self { Self }
    fn label(self, _: impl Into<String>) -> Self { self }
}

fn main() {
    let _ = ui! { Button("save") { label: 3 } };
}
