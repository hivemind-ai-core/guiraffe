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
        vcx.update(|window, cx| {
            window.render_frame(cx);
            let root = window.find("event-timeline").bounds();
            let target = window.find(("event-timeline-item", 998u64)).bounds();
            assert!(root.contains(&target.center()));
        });
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

    use gpui_kit::{KeyUpEvent, Keystroke, PlatformInput, ScrollDelta, ScrollEvent, point};
    use guiraffe_timeline::{EventLink, LinkKind};

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

    fn thread_cpu_ns() -> u64 {
        std::fs::read_to_string("/proc/thread-self/schedstat")
            .ok()
            .and_then(|status| status.split_whitespace().next()?.parse().ok())
            .unwrap_or_else(|| panic!("thread CPU time unavailable"))
    }

    fn record(histogram: &mut [u64; 1001], elapsed: Duration) {
        let bucket = (elapsed.as_micros() / 100).min(1000) as usize;
        if let Some(bin) = histogram.get_mut(bucket) {
            *bin += 1;
        }
    }

    fn p99_ms(histogram: &[u64; 1001], samples: u64) -> f64 {
        let mut cumulative = 0;
        let bucket = histogram
            .iter()
            .position(|count| {
                cumulative += count;
                cumulative * 100 >= samples * 99
            })
            .unwrap_or(1000);
        (bucket + 1) as f64 / 10.0
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
    let mount_started = Instant::now();
    let tracks: Vec<_> = (0..4)
        .map(|id| EventTrack {
            id,
            label: format!("Lane {id}"),
            color: None,
        })
        .collect();
    let events: Vec<_> = (0..count)
        .map(|id| TimelineEvent {
            id,
            track: id % 4,
            position: (id / 8) as i64,
            label: format!("Event {id}"),
            content: Some(format!("Completed stage {}", id % 11)),
            tags: vec![],
            color: None,
        })
        .collect();
    let links: Vec<_> = (0..count - 17)
        .step_by(32)
        .map(|source| EventLink {
            source,
            destination: source + 17,
            kind: if source % 64 == 0 {
                LinkKind::Branch
            } else {
                LinkKind::Merge
            },
            color: None,
        })
        .collect();
    let raw_bytes = std::mem::size_of_val(tracks.as_slice())
        + tracks
            .iter()
            .map(|track| track.label.capacity())
            .sum::<usize>()
        + std::mem::size_of_val(events.as_slice())
        + events
            .iter()
            .map(|event| {
                event.label.capacity() + event.content.as_ref().map_or(0, String::capacity)
            })
            .sum::<usize>()
        + std::mem::size_of_val(links.as_slice());
    let model = EventModel::new(tracks, events, links).unwrap_or_else(|error| panic!("{error}"));
    entity.update(vcx, |timeline, cx| timeline.replace(model, cx));
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
    let mut wall = [0; 1001];
    let mut cpu = [0; 1001];
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
        let cpu_start = thread_cpu_ns();
        let frame = Instant::now();
        vcx.update(|window, cx| {
            if samples % 120 == 30 {
                let viewport = window.viewport_size();
                window.dispatch_event(
                    PlatformInput::Scroll(ScrollEvent {
                        position: point(viewport.width / 2.0, viewport.height / 2.0),
                        delta: ScrollDelta::Pixels(point(px(0.), px(-540.))),
                        ..Default::default()
                    }),
                    cx,
                );
            } else {
                let keystroke = Keystroke::parse_with_os(key, cx.platform_info().os)
                    .unwrap_or_else(|error| panic!("{error}"));
                window.dispatch_keystroke(keystroke.clone(), cx);
                window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke }), cx);
            }
        });
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let elapsed = frame.elapsed();
        record(&mut wall, elapsed);
        record(
            &mut cpu,
            Duration::from_nanos(thread_cpu_ns().saturating_sub(cpu_start)),
        );
        if samples % 120 == 0 {
            last_rss = rss_bytes();
            peak = peak.max(last_rss);
            if start.elapsed() > duration.min(Duration::from_secs(30)) / 2 {
                warmed.get_or_insert(last_rss);
            }
        }
        samples += 1;
    }
    let wall_p99 = p99_ms(&wall, samples);
    let cpu_p99 = p99_ms(&cpu, samples);
    println!(
        "mount_ms={:.2} p99_frame_ms={wall_p99:.2} p99_thread_cpu_ms={cpu_p99:.2} samples={samples} duration_s={:.2} raw_bytes={raw_bytes} baseline_rss={baseline} peak_rss={peak} warm_rss={warmed:?} final_rss={last_rss}",
        mount.as_secs_f64() * 1000.0,
        start.elapsed().as_secs_f64()
    );
    assert!(mount <= Duration::from_millis(200));
    assert!(wall_p99 <= 8.0);
    assert!(peak.saturating_sub(baseline) <= raw_bytes * 5 / 4 + 64 * 1024 * 1024);
}
