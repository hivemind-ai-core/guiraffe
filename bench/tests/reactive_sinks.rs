//! Fixed-metric ASCII label prototype. Each retained row owns a display-list
//! slice with stable coordinates. A binding records the slice at mount time.
use std::hint::black_box;

#[derive(Clone, Copy)]
struct Binding {
    row: usize,
    glyph_start: usize,
}

struct Scene {
    model: Vec<[u8; 8]>,
    glyphs: Vec<(usize, u8)>,
    bindings: Vec<Binding>,
}

impl Scene {
    fn mount(rows: usize) -> Self {
        let model = vec![*b"original"; rows];
        let glyphs = model
            .iter()
            .flatten()
            .enumerate()
            .map(|(position, glyph)| (position, *glyph))
            .collect();
        let bindings = (0..rows)
            .map(|row| Binding {
                row,
                glyph_start: row * 8,
            })
            .collect();
        Self {
            model,
            glyphs,
            bindings,
        }
    }

    fn write(&mut self, key: usize, label: [u8; 8]) {
        let binding = self.bindings[key];
        if self.model[binding.row] == label {
            return;
        }
        self.model[binding.row] = label;
        for (command, glyph) in self.glyphs[binding.glyph_start..][..8]
            .iter_mut()
            .zip(label)
        {
            command.1 = glyph;
        }
    }
}

fn sweep(rows: usize) {
    let mut scene = Scene::mount(rows);
    for i in 0..1_000_000 {
        let key = i % rows;
        let label = if (i / rows) % 2 == 0 {
            *b"modified"
        } else {
            *b"original"
        };
        black_box(&mut scene).write(key, label);
    }
    let index = (999_999 % rows) * 8;
    assert_eq!(scene.glyphs[index].1, scene.model[index / 8][0]);
}

#[test]
fn fixed_label_dirty_replay() -> anyhow::Result<()> {
    use guiraffe_bench::allocator::{CountingAllocator, Counts, count_allocations};
    use nix::sys::time::TimeValLike;
    use nix::time::{ClockId, clock_gettime};
    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;
    for rows in [1000, 3000] {
        let mut scene = Scene::mount(rows);
        let (_, allocations) = count_allocations(|| scene.write(rows / 2, *b"modified"))?;
        assert_eq!(allocations, Counts::default());
        for (index, command) in scene.glyphs.iter().enumerate() {
            let expected = if index / 8 == rows / 2 {
                b"modified"
            } else {
                b"original"
            };
            assert_eq!(*command, (index, expected[index % 8]));
        }
        let start = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID)?;
        sweep(rows);
        let elapsed = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID)? - start;
        eprintln!(
            "{rows} rows: {:.1} ns CPU/edit including mount amortized",
            elapsed.num_nanoseconds() as f64 / 1e6
        );
    }
    Ok(())
}
