//! A small live "event log" panel: demos push a line per emitted event so users see what
//! fires, in order, without a debugger.

use std::collections::VecDeque;

use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window, div, px,
};
use guiraffe_components::Text;
use guiraffe_core::{GuiraffeTheme, Size};

/// Lines kept; older events scroll off.
pub const CAPACITY: usize = 5;

/// The pure ring of recent events, newest first, with a running count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventLines {
    lines: VecDeque<SharedString>,
    count: usize,
}

impl EventLines {
    pub fn push(&mut self, line: impl Into<SharedString>) {
        self.count += 1;
        self.lines.push_front(line.into());
        self.lines.truncate(CAPACITY);
    }

    pub fn lines(&self) -> impl Iterator<Item = &SharedString> {
        self.lines.iter()
    }

    pub fn count(&self) -> usize {
        self.count
    }
}

/// The panel entity. Clone the handle into callbacks and call [`EventLog::log`].
pub struct EventLog {
    lines: EventLines,
    _subs: Vec<Subscription>,
}

impl EventLog {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            lines: EventLines::default(),
            _subs: Vec::new(),
        })
    }

    pub fn lines(&self) -> &EventLines {
        &self.lines
    }

    /// Append `line` and repaint only the log.
    pub fn log(this: &Entity<Self>, line: impl Into<SharedString>, cx: &mut App) {
        let line = line.into();
        this.update(cx, |l, cx| {
            l.lines.push(line);
            cx.notify();
        });
    }

    /// Log every event `emitter` emits, formatted with `{:?}`.
    pub fn watch<E: 'static, T: EventEmitter<E>>(
        this: &Entity<Self>,
        emitter: &Entity<T>,
        cx: &mut App,
    ) where
        E: std::fmt::Debug,
    {
        Self::watch_with(this, emitter, |e: &E| format!("{e:?}").into(), cx);
    }

    /// Log events from `emitter` through `fmt`.
    pub fn watch_with<E: 'static, T: EventEmitter<E>>(
        this: &Entity<Self>,
        emitter: &Entity<T>,
        fmt: impl Fn(&E) -> SharedString + 'static,
        cx: &mut App,
    ) {
        let emitter = emitter.clone();
        this.update(cx, |l, cx| {
            let sub = cx.subscribe(&emitter, move |l: &mut EventLog, _, e: &E, cx| {
                l.lines.push(fmt(e));
                cx.notify();
            });
            l._subs.push(sub);
        });
    }
}

impl Render for EventLog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = GuiraffeTheme::global(cx).surfaces().dimmed.to_hsla();
        let body: AnyElement = if self.lines.count() == 0 {
            Text::new("Interact with the example; events appear here.")
                .size(Size::Xs)
                .dimmed(true)
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .font_family("monospace")
                .text_size(px(Size::Xs.font_size()))
                .children(self.lines.lines().enumerate().map(|(i, l)| {
                    let row = div().child(l.clone());
                    if i == 0 { row } else { row.text_color(muted) }
                }))
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                Text::new(format!("Event log · {} fired", self.lines.count()))
                    .size(Size::Xs)
                    .dimmed(true),
            )
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_newest_first_and_counts_all() {
        let mut l = EventLines::default();
        for i in 0..8 {
            l.push(format!("e{i}"));
        }
        assert_eq!(l.count(), 8);
        let v: Vec<_> = l.lines().map(|s| s.to_string()).collect();
        assert_eq!(v, ["e7", "e6", "e5", "e4", "e3"]);
    }
}
