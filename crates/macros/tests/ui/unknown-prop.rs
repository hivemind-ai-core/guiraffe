use guiraffe_macros::ui;

struct Button;
impl Button {
    fn new() -> Self { Self }
}

fn main() {
    let _ = ui! { Button { lable: "Save" } };
}
