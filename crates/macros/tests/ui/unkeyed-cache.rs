use guiraffe_macros::ui;

fn main() {
    let _ = ui! {
        for row in [1, 2] {
            div {
                if row > 0 { cached(style) { Text(row) } }
            }
        }
    };
}
