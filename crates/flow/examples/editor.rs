use gpui_kit::*;
use guiraffe_flow::{Flow, core::*};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = Graph::from_parts(
        [
            Node::new(1, "Input", guiraffe_flow::core::Point::new(100., 160.)),
            Node::new(2, "Transform", guiraffe_flow::core::Point::new(420., 160.)),
            Node::new(3, "Output", guiraffe_flow::core::Point::new(740., 300.)),
        ],
        [
            Edge {
                label: "signal".into(),
                animated: true,
                ..Edge::new(1, 1, 2)
            },
            Edge::new(2, 2, 3),
        ],
    )?;
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let result = gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|cx| match Flow::new(graph, cx) {
                Ok(flow) => flow,
                Err(error) => panic!("validated demo: {error}"),
            })
        });
        if let Err(error) = result {
            eprintln!("{error}");
        }
    });
    Ok(())
}
