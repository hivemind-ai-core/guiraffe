//! A custom background layer and external curve editing, without recreating axis logic.
use gpui_kit::{AppContext, WindowOptions};
use guiraffe_timeline::*;
use std::rc::Rc;
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::component::init(cx);
            guiraffe_core::apply(Default::default(), cx);
            let result = cx.open_window(WindowOptions::default(), |window, cx| {
                let mut curve = Curve::default();
                curve.set(Key::new(0., 0., Interp::Linear));
                curve.set(Key::new(1., 1., Interp::Bezier));
                let view = CurveView::new(curve, 0., 2., -0.2, 1.2, cx);
                view.update(cx, |view, cx| {
                    view.add_layer(
                        BackgroundBands {
                            intervals: Rc::from([(1., 1.5)]),
                            ink: plot::Ink::Stripe,
                        },
                        cx,
                    );
                    view.insert_at(0.5, cx);
                    view.select_key(0, 1, cx);
                });
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            });
            if let Err(error) = result {
                eprintln!("Cannot open plot example: {error}");
                cx.quit();
            }
        });
}
