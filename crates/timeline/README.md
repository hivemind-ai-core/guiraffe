# guiraffe-timeline

Time-based plots and editors on `PlotSurface`, with pure axis, curve, channel-selection, clip and schedule models. Initialize Kit with `gpui_kit::component::init(cx)` and apply a `GuiraffeTheme` before constructing native views.

`PlotSurface` has an x `Axis`, optional y `ValueRange` and ordered `Layer` stack. Custom layers append visible-coordinate `plot::Primitive` commands in `Layer::prepaint`; the native canvas paints them in order. Built-ins are `Ruler`, `Gridlines`, `GridMarks`, `Playhead`, `SelectionBox`, `Markers`, `Zebra` and `BackgroundBands`. `add_layer`, `insert_layer` and `clear_layers` control composition. `navigate` implements pan/anchored zoom, and `fit` frames a range. Ruler layers target a separate header, sharing the body axis.

All editor views accept a layer with one call:

```rust,ignore
view.update(cx, |view, cx| view.add_layer(Gridlines::default(), cx));
```

Gantt and Tracks retain their scheduling/clip APIs. Global `set_editable(false, cx)` prevents gestures and leaves item appearance alone. Item locks remain independent; label-column Kit icon buttons toggle them. `set_lock_style(item, global, cx)` configures `None`, `Badge`, `Dimmed` or `Hatched`; defaults are an item badge with disabled theme tokens and no global styling. Lock styling never changes edit admission.

## TracksView

The model is store state. `TracksModel` holds the track rows (name, clips, height), the selection, the playhead, the follow mode and the view range (`ViewRange { start, scale }`, seconds per pixel, independent of width). `tracks_store(model, undo, cx)` makes a `Store` with undo over the rows only. `TracksView::with_store(store, format, length, cx)` renders it; `TracksView::new(tracks, format, length, cx)` makes its own store.

- **Host writes** are store events: `SetTracks(Vec<TrackUpdate>)` (diffed by index: unchanged rows keep their allocation and height, the selection survives while its clip exists, the view range is untouched), `SetPlayhead(t)`, `SetView(range)`, `SetFollow(Follow::{Off, Page, Center})`, `SetMinTime(Some(0.0))`, `SetTrackHeight(track, h)`, `SetSelection(..)`. They never reach the bus and never emit a `TracksEvent`, so there is nothing to filter.
- **User edits** are `TracksAction`s: `ResizeTrack`, `MoveClip`, `TrimClip` (the end may extend past the content; a clip's length is host data), `Select` and `Seek`. `TracksView::register(&view, path, window, cx)` puts the view on the action bus; pointer and keyboard input then dispatch the same erased action a bus client sends (`resize_track`, `move_clip`, `trim_clip`, `select`, `seek`), tagged with its `Source`. A drag previews locally and commits one action on release as a `dispatch_batch`, so it is one undo step. Each applied action emits `TracksEvent` (`ClipChanged { edit: Move | Trim(edge) }`, `TrackResized`, `SelectionChanged`, `Seek`) and returns its `TracksChange` as the bus outcome. `perform(action, cx)` applies one directly.
- **Rows** have their own heights (`TrackRow::with_height`, clamped to 20..=800 px). Layout is a prefix sum (`RowLayout`), and hit testing a binary search over row tops. `set_resizable_tracks(true, cx)` lets the user drag a row's bottom edge. A vertical wheel scrolls the rows when they overflow (Shift pans time); `scroll_y`/`set_scroll_y` read and set it.
- **Clip content:** `set_clip_painter(Some(Rc<dyn ClipPainter>), cx)`. `ClipPainter::paint(&ClipPaint, window, &App)` gets a snapshot of one visible clip (track, row, clip, on-screen bounds, the axis mapping time to x, selected) and only paints. It runs for visible clips of rows below overview density, after the clip background and before the selection.
- **Selection** is an accent border with a light tint (`SelectionStyle::themed`); `set_selection_style(Some(style), cx)` overrides it.
- **Header column:** `set_header(HeaderMode::Default | Hidden | Custom(width, renderer), cx)`. A custom renderer gets a `HeaderRow` (index, row, height, selected) per visible row and is sized and aligned to it.

Not yet: a clip-content pointer handler (first refusal on pointer events inside a clip). Painters only paint.

Dope sheets accept `Channel::group(name, children)` and `Channel::with_keyframes(name, keys)` as well as time-only channels. Group rows aggregate descendant times as neutral diamonds without value hold bars; channel controls expand/collapse, mute, lock and hide keys, with theme-colored channel swatches. Only visible rows and horizontally visible key glyphs instantiate/paint. Click selects, Shift-click toggles selection, dragging moves selected editable keys together with frame snapping, and dragging empty space selects a box. Collisions reject the entire move. `set_focusable(true, cx)` opts into Tab focus and keyboard editing. Pointer presses preserve existing focus. Once focused, Ctrl/Command+A selects all, Ctrl/Command+D duplicates one frame later, arrows nudge, and Delete/Backspace removes selected editable keys. Group collapse retains selection. Mute/visibility are host-readable channel state; this crate does not evaluate animation playback.

Curve controls can live outside the display:

```rust,ignore
view.update(cx, |view, cx| {
    view.insert_at(time, cx); // samples and splits the current segment
    view.set_interpolation(Interp::Bezier, cx);
    view.set_tangent_mode(TangentMode::Aligned);
    view.set_tangent(false, (0.2, 0.4), cx);
});
```

`add_key`, `remove_selected`, `split_segment`, `remove_segment`, `select_key`, `add_curve`, `set_curve_visible` and `fit_view` cover external toolbars. `remove_segment(i)` removes its end key and joins neighbours with the preceding interpolation mode. Cubic splitting preserves the original curve. Selected Bezier keys show draggable handles, including pickable defaults for zero-length tangents; Free/Aligned/Mirrored/Flat modes determine the opposite handle. Fit with normalization adjusts the shared value viewport; it does not rewrite curve values.

`BarCurve::new(n, min, max)` stores N bounded values; `set_steps(Some(n))` quantizes to N intervals and `bipolar=true` draws around zero. `draw(from, to)` fills every intervening bar in either direction and returns the changed index range. `BarCurveView` emits `BarEvent` for pointer drawing, and `from_curve`/`to_curve` sample between finite time endpoints. Render captures share the values; edits copy on write if a previous frame retains them.

See `examples/plot_layers.rs` and the five showcase timeline entries.
