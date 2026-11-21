use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Path, Render, StyleRefinement, Styled,
    TestAppContext, Window, canvas, point, px, rgb,
};
use guiraffe_bench::allocator::{CountingAllocator, Counts, count_allocations};
use std::{cell::Cell, rc::Rc};

const PATH_COUNT: usize = 1000;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Paths(Rc<Cell<usize>>);

impl Render for Paths {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let paints = self.0.clone();
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                paints.set(paints.get() + 1);
                for index in 0..PATH_COUNT {
                    let origin = bounds.origin
                        + point(px((index % 40) as f32 * 4.), px((index / 40) as f32 * 4.));
                    let mut path = Path::new(origin);
                    path.push_triangle(
                        (
                            origin,
                            origin + point(px(3.), px(0.)),
                            origin + point(px(0.), px(3.)),
                        ),
                        (point(0., 0.), point(0., 0.), point(0., 0.)),
                    );
                    window.paint_path(path, rgb(0x0055aa));
                }
            },
        )
        .size_full()
    }
}

struct Root(Entity<Paths>);

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0
            .clone()
            .cached(StyleRefinement::default().w(px(180.)).h(px(120.)))
    }
}

#[gpui_kit::test]
fn cached_paths_repaint_without_allocating(cx: &mut TestAppContext) {
    let paints = Rc::new(Cell::new(0));
    let (_, window) = cx.add_window_view(|_, cx| Root(cx.new(|_| Paths(paints.clone()))));
    window.run_until_parked();
    window.update(|window, cx| {
        for _ in 0..8 {
            window.draw(cx).clear(cx);
        }
        let initial_paints = paints.get();
        assert!(initial_paints > 0);
        assert_eq!(window.painted_path_count(), PATH_COUNT);
        let frames = std::env::var("GUIRAFFE_CACHED_PATH_FRAMES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(8)
            .max(1);
        let (_, counts) = count_allocations(|| {
            for _ in 0..frames {
                window.draw(cx).clear(cx);
            }
        })
        .expect("count cached repaints");
        assert_eq!(window.painted_path_count(), PATH_COUNT);
        assert_eq!(
            paints.get(),
            initial_paints,
            "the cached view repainted its canvas"
        );
        assert_eq!(counts, Counts::default());
    });
}

#[test]
fn path_clones_share_vertices_until_mutated() {
    let origin = point(px(0.), px(0.));
    let mut path = Path::new(origin);
    path.push_triangle(
        (origin, point(px(3.), px(0.)), point(px(0.), px(3.))),
        (point(0., 0.), point(0., 0.), point(0., 0.)),
    );
    let original = path.scale(2.);
    let mut copy = original.clone();
    assert_eq!(copy.vertices.as_ptr(), original.vertices.as_ptr());

    copy.vertices[0].xy_position.x = original.vertices[1].xy_position.x;
    assert_ne!(copy.vertices.as_ptr(), original.vertices.as_ptr());
    assert_ne!(copy.vertices[0].xy_position, original.vertices[0].xy_position);
}
