//! A flat-colored frame row needs only one device scanline. Stretch that
//! scanline to the row height; labels and sparsely occupied rows stay on GPUI's
//! quad path. Hit testing always uses the model.
use crate::{ColourScheme, FlameLayout, Search, view::palette_index};
use gpui_kit::{App, Bounds, Pixels, RenderImage, Window};
use guiraffe_core::Rgb;
use std::{ops::Range, sync::Arc};

const MAX_ROWS: usize = 128;
const MAX_PIXELS: usize = 4096;
const MAX_COLOR_ENTRIES: usize = 4096;

pub(super) struct Projection {
    pub layout: Arc<FlameLayout>,
    pub search: Option<Arc<Search>>,
    pub domain: Range<f64>,
    pub first_row: usize,
    pub colors: [Rgb; 7],
    pub scheme: ColourScheme,
    pub width: f64,
    pub scale: f32,
    pub origin_x: f32,
}

impl Projection {
    fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.layout, &other.layout)
            && self.first_row == other.first_row
            && self.scheme == other.scheme
    }

    fn same_view(&self, other: &Self) -> bool {
        self.domain == other.domain
            && self.width == other.width
            && self.scale == other.scale
            && self.origin_x == other.origin_x
            && self.colors == other.colors
            && match (&self.search, &other.search) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }

    // Match GPUI's endpoint snapping, including half-pixels at fractional DPI.
    fn column(&self, x: f64) -> usize {
        let snap = |v: f32| (v.abs() - 0.5).ceil().copysign(v);
        let origin = self.origin_x * self.scale;
        (snap((self.origin_x + x as f32) * self.scale) - snap(origin)) as usize
    }
}

#[derive(Default)]
struct Row {
    indices: Vec<u8>,
    image: Option<Arc<RenderImage>>,
    current: bool,
}

impl Row {
    fn color_index(&mut self, view: &Projection, offset: usize, source: usize) -> usize {
        if offset >= MAX_COLOR_ENTRIES {
            return palette_index(view.layout.source_frame(source), view.scheme);
        }
        if self.indices.len() <= offset {
            self.indices.resize(offset + 1, u8::MAX);
        }
        if self.indices[offset] == u8::MAX {
            self.indices[offset] =
                palette_index(view.layout.source_frame(source), view.scheme) as u8;
        }
        usize::from(self.indices[offset])
    }

    fn rasterize(
        &mut self,
        view: &Projection,
        slot: usize,
        width: usize,
        pixels: &mut Vec<[u8; 4]>,
    ) {
        pixels.clear();
        pixels.resize(width, [0; 4]);
        view.layout.visit_indexed_row(
            view.first_row + slot,
            view.domain.clone(),
            view.width,
            view.search.as_deref(),
            |single, x, highlight| {
                let color = if highlight {
                    view.colors[6]
                } else {
                    let index =
                        single.map_or(0, |(offset, source)| self.color_index(view, offset, source));
                    view.colors[index]
                };
                let start = view.column(x.start).min(width);
                let end = view.column(x.end).min(width);
                pixels[start..end].fill([color.b, color.g, color.r, color.a]);
            },
        );
    }
}

#[derive(Default)]
pub(super) struct RowStrips {
    rows: Vec<Row>,
    pixels: Vec<[u8; 4]>,
    projection: Option<Projection>,
}

impl RowStrips {
    pub fn prepare(&mut self, next: Projection) {
        if let Some(old) = &self.projection {
            let same_source = old.same_source(&next);
            if same_source && old.same_view(&next) {
                return;
            }
            for row in &mut self.rows {
                row.current = false;
                if !same_source {
                    row.indices.clear();
                }
            }
        }
        self.projection = Some(next);
    }

    pub fn release(&mut self, cx: &mut App) {
        for image in self.rows.drain(..).filter_map(|row| row.image) {
            cx.drop_image(image, None);
        }
    }

    pub fn paint(
        &mut self,
        slot: usize,
        rect: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(view) = &self.projection else {
            return false;
        };
        let width = view.column(view.width);
        if slot >= MAX_ROWS
            || !(1..=MAX_PIXELS).contains(&width)
            || !view.layout.has_dense_unlabelled_row(
                view.first_row + slot,
                &view.domain,
                view.width,
            )
        {
            return false;
        }
        if self.rows.len() <= slot {
            self.rows.resize_with(slot + 1, Row::default);
        }
        let row = &mut self.rows[slot];
        if !row.current {
            row.rasterize(view, slot, width, &mut self.pixels);
            let bytes = self.pixels.as_flattened();
            if row.image.as_ref().and_then(|image| image.as_bytes(0)) != Some(bytes) {
                let Some(buffer) = image::RgbaImage::from_vec(width as u32, 1, bytes.to_vec())
                else {
                    return false;
                };
                let image = Arc::new(RenderImage::new([image::Frame::new(buffer)]));
                if let Some(old) = row.image.replace(image) {
                    cx.drop_image(old, Some(window));
                }
            }
            row.current = true;
        }
        row.image.as_ref().is_some_and(|image| {
            window
                .paint_image(rect, rect, Default::default(), image.clone(), 0, false)
                .is_ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Frame, FrameSource};

    struct Stacks;
    impl FrameSource for Stacks {
        fn len(&self) -> usize {
            128
        }
        fn frame(&self, index: usize) -> Frame<'_> {
            // Half of each stack has a child; the other half of the child row is a gap.
            let child = index % 2 == 1;
            Frame {
                id: 1000 + index as u64,
                parent: child.then(|| 999 + index as u64),
                self_value: 1.,
                total: if child { 1. } else { 2. },
                label: if index % 4 == 1 { "match" } else { "rest" },
                package: "demo",
            }
        }
    }

    #[test]
    fn raster_preserves_gaps_matches_and_device_pixel_boundaries() {
        let layout = Arc::new(FlameLayout::new(Arc::new(Stacks)).expect("valid stacks"));
        let mut view = Projection {
            search: layout.search("match").map(Arc::new),
            domain: layout.domain(),
            layout,
            first_row: 0,
            colors: std::array::from_fn(|i| Rgb::hex(i as u32 + 1)),
            scheme: ColourScheme::Name,
            width: 128.,
            scale: 2.,
            origin_x: 0.25,
        };
        let mut row = Row::default();
        let mut pixels = Vec::new();
        for width in [128., 40.] {
            view.width = width;
            let count = view.column(width);
            row.rasterize(&view, 1, count, &mut pixels);
            let mut expected = vec![[0; 4]; count];
            view.layout.visit_paint_row(
                1,
                view.domain.clone(),
                width,
                view.search.as_deref(),
                |seg| {
                    let i = if seg.highlighted {
                        6
                    } else {
                        seg.id
                            .and_then(|id| view.layout.frame(id))
                            .map_or(0, |frame| palette_index(frame, view.scheme))
                    };
                    let c = view.colors[i];
                    expected[view.column(seg.x.start)..view.column(seg.x.end)]
                        .fill([c.b, c.g, c.r, c.a]);
                },
            );
            assert_eq!(pixels, expected);
            if width == 128. {
                assert!(pixels.contains(&[0; 4]));
                assert!(pixels.contains(&[7, 0, 0, 255]));
            }
        }
    }
}
