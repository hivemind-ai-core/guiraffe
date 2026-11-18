use guiraffe_flamegraph::{FlameLayout, Frame, FrameSource};
use proptest::prelude::*;
use std::sync::Arc;

struct Profile(usize);

impl FrameSource for Profile {
    fn len(&self) -> usize {
        self.0 + 1
    }
    fn frame(&self, i: usize) -> Frame<'_> {
        if i == 0 {
            Frame {
                id: 0,
                parent: None,
                self_value: 0.,
                total: self.0 as f64,
                label: "root",
                package: "",
            }
        } else {
            Frame {
                id: i as u64,
                parent: Some(0),
                self_value: 1.,
                total: 1.,
                label: "child",
                package: "",
            }
        }
    }
}

proptest! {
    #[test]
    fn hits_focus_and_culling_agree(count in 1usize..500, start in 0usize..500, length in 1usize..500) {
        let layout = FlameLayout::new(Arc::new(Profile(count))).map_err(|e| TestCaseError::fail(format!("{e:?}")))?;
        let end = (start + length).min(count);
        let start = start.min(end);
        let mut visible = Vec::new();
        layout.visit_row(1, start as f64..end as f64, ((end-start)*2) as f64, None, |s| visible.push(s.id));
        prop_assert_eq!(visible, (start..end).map(|i| Some(i as u64 + 1)).collect::<Vec<_>>());
        for index in start..end {
            let id = index as u64 + 1;
            prop_assert_eq!(layout.hit(1, index as f64 + 0.5), Some(id));
            prop_assert_eq!(layout.focus(id), Some(index as f64..index as f64 + 1.));
            prop_assert_eq!(layout.click(id).map(|event| event.path), Some(vec![0,id]));
        }
    }
}

#[test]
fn million_frame_overview_bounds_bucket_count() -> Result<(), String> {
    let layout = FlameLayout::new(Arc::new(Profile(1_000_000))).map_err(|e| format!("{e:?}"))?;
    let search = layout.search("child");
    let mut count = 0;
    layout.visit_row(1, layout.domain(), 1000., search.as_ref(), |segment| {
        count += 1;
        assert!(segment.highlighted);
        assert!(segment.x.start >= 0. && segment.x.end <= 1000.);
    });
    assert!((1..=1000).contains(&count));
    Ok(())
}
