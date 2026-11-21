use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Role, Styled, TestAppContext, Window, div, px, test::TestWindowExt,
};
use guiraffe_timeline::{
    EventModel, EventOrientation, EventTimeline, EventTimelineEvent, EventTimelineState,
    EventTrack, TimelineEvent,
};

struct Host {
    timeline: Entity<EventTimelineState>,
    previous: FocusHandle,
    activated: Rc<Cell<Option<u64>>>,
    _subscription: gpui_kit::Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("before")
                    .track_focus(&self.previous)
                    .h(px(24.))
                    .child("Before"),
            )
            .child(div().h(px(240.)).child(EventTimeline::new(&self.timeline)))
    }
}

fn model(count: u64) -> EventModel {
    let tracks = (0..2)
        .map(|id| EventTrack {
            id,
            label: format!("Lane {id}"),
            color: None,
        })
        .collect();
    let events = (0..count)
        .map(|id| TimelineEvent {
            id,
            track: id % 2,
            position: (id / 2) as i64,
            label: format!("Event {id}"),
            content: None,
            tags: vec![],
            color: None,
        })
        .collect();
    EventModel::new(tracks, events, vec![]).unwrap_or_else(|error| panic!("{error}"))
}

#[gpui_kit::test]
fn navigation_reveals_offscreen_events_and_pointer_preserves_focus(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guiraffe_core::init(cx);
        guiraffe_timeline::init(cx);
    });
    for orientation in [EventOrientation::Vertical, EventOrientation::Horizontal] {
        let rendered = Rc::new(Cell::new(0));
        let slot_calls = rendered.clone();
        let (host, vcx) = cx.add_window_view(move |_, cx| {
            let timeline = cx.new(|cx| {
                EventTimelineState::new(model(1_000), cx)
                    .orientation(orientation)
                    .focusable(true)
                    .item_content(move |_, _, _| {
                        slot_calls.set(slot_calls.get() + 1);
                        div().child("Custom content").into_any_element()
                    })
            });
            let activated = Rc::new(Cell::new(None));
            let last_activation = activated.clone();
            let subscription = cx.subscribe(&timeline, move |_, _, event, _| {
                if let &EventTimelineEvent::Activated(id) = event {
                    last_activation.set(Some(id));
                }
            });
            Host {
                timeline,
                previous: cx.focus_handle().tab_stop(true),
                activated,
                _subscription: subscription,
            }
        });
        vcx.run_until_parked();
        assert!(rendered.get() > 0 && rendered.get() < 200);
        let (timeline, previous, activated) = host.read_with(vcx, |host, _| {
            (
                host.timeline.clone(),
                host.previous.clone(),
                host.activated.clone(),
            )
        });
        let original_bounds = vcx.update(|window, cx| {
            window.focus(&previous, cx);
            window.render_frame(cx);
            let bounds = window.find(("event-timeline-item", 0usize)).bounds();
            window.click(("event-timeline-item", 0usize), cx);
            assert!(previous.is_focused(window));
            window.focus_next(cx);
            assert!(timeline.read(cx).focus_handle(cx).is_focused(window));
            bounds
        });
        vcx.update(|window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("event-timeline").role(), Some(Role::ListBox));
            let option = window.find(("event-timeline-item", 0usize));
            assert_eq!(option.role(), Some(Role::ListBoxOption));
            assert_eq!(option.bounds(), original_bounds);
            assert_eq!(option.selected(), Some(true));
            assert!(
                option
                    .label()
                    .is_some_and(|label| label.starts_with("Event 0"))
            );
        });
        vcx.simulate_keystrokes(match orientation {
            EventOrientation::Vertical => "down",
            EventOrientation::Horizontal => "right",
        });
        assert_eq!(
            timeline.read_with(vcx, |timeline, _| timeline.selected()),
            Some(2)
        );
        vcx.simulate_keystrokes("end enter");
        assert_eq!(activated.get(), Some(998));
        timeline.update(vcx, |timeline, cx| timeline.replace(model(1_000), cx));
        assert_eq!(
            timeline.read_with(vcx, |timeline, _| timeline.selected()),
            Some(998)
        );
        assert!(rendered.get() < 500, "content projected for distant items");
    }
}

#[cfg(target_os = "linux")]
#[gpui_kit::test]
#[ignore = "release-mode 100k-event soak; run explicitly"]
fn soak_100k(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};

    fn rss_bytes() -> usize {
        let status =
            std::fs::read_to_string("/proc/self/status").unwrap_or_else(|error| panic!("{error}"));
        status
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")?
                    .split_whitespace()
                    .next()?
                    .parse::<usize>()
                    .ok()
            })
            .unwrap_or_else(|| panic!("VmRSS unavailable"))
            * 1024
    }

    cx.update(|cx| {
        guiraffe_core::init(cx);
        guiraffe_timeline::init(cx);
    });
    let (entity, vcx) =
        cx.add_window_view(|_, cx| EventTimelineState::new(model(0), cx).focusable(true));
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));
    let baseline = rss_bytes();
    let count = 100_000;
    let raw_bytes = count as usize * std::mem::size_of::<TimelineEvent>()
        + (0..count)
            .map(|id| format!("Event {id}").capacity())
            .sum::<usize>()
        + 2 * (std::mem::size_of::<EventTrack>() + "Lane 0".len());
    let mount_started = Instant::now();
    entity.update(vcx, |timeline, cx| timeline.replace(model(count), cx));
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.focus(&entity.read(cx).focus_handle(cx), cx);
    });
    let mount = mount_started.elapsed();
    let duration = Duration::from_secs(
        std::env::var("EVENT_TIMELINE_SOAK_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1800),
    );
    let mut histogram = [0u64; 1001];
    let start = Instant::now();
    let mut samples = 0;
    let mut peak = rss_bytes();
    let mut warmed = None;
    let mut last_rss = peak;
    while start.elapsed() < duration || samples < 1_000 {
        let key = match samples % 120 {
            0 => "home",
            60 => "end",
            1..60 => "down",
            _ => "up",
        };
        let frame = Instant::now();
        vcx.update(|window, cx| {
            window.press(key, cx);
            window.render_frame(cx);
        });
        let bucket = (frame.elapsed().as_micros() / 100).min(1000) as usize;
        if let Some(bin) = histogram.get_mut(bucket) {
            *bin += 1;
        }
        if samples % 120 == 0 {
            last_rss = rss_bytes();
            peak = peak.max(last_rss);
            if start.elapsed() > duration.min(Duration::from_secs(30)) / 2 {
                warmed.get_or_insert(last_rss);
            }
        }
        samples += 1;
    }
    let mut cumulative = 0;
    let bucket = histogram
        .iter()
        .position(|count| {
            cumulative += count;
            cumulative * 100 >= samples * 99
        })
        .unwrap_or(1000);
    let p99_ms = (bucket + 1) as f64 / 10.0;
    println!(
        "mount_ms={:.2} p99_cpu_ms={p99_ms:.2} samples={samples} duration_s={:.2} raw_bytes={raw_bytes} baseline_rss={baseline} peak_rss={peak} warm_rss={warmed:?} final_rss={last_rss}",
        mount.as_secs_f64() * 1000.0,
        start.elapsed().as_secs_f64()
    );
    assert!(mount <= Duration::from_millis(200));
    assert!(p99_ms <= 8.0);
    assert!(peak.saturating_sub(baseline) <= raw_bytes * 5 / 4 + 64 * 1024 * 1024);
}
