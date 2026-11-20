use guiraffe_macros::ui;

fn main() {
    let _ = ui! { div { for row in rows { Button(row) {} } } };
}
