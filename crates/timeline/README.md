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

- **Tracks are keyed.** Every `TrackRow` and `TrackUpdate` carries a host-provided `TrackId` (Passage's track id); `ClipRef { track: TrackId, clip }` names a clip by track key and clip id, never by position. `TracksModel::index_of(id)` and `selected_index()` give display positions.
- **Host writes** are store events: `SetTracks(Vec<TrackUpdate>)` (diffed by key: an unchanged key keeps its allocation, a known key keeps its height wherever it moves, inserts and removes touch only those rows, the selection follows its track's key, and the view range is untouched), `SetPlayhead(t)`, `SetView(range)`, `SetFollow(Follow::{Off, Page, Center})`, `SetMinTime(Some(0.0))`, `SetTrackHeight(track, h)`, `SetSelection(..)`. They never reach the bus and never emit a `TracksEvent`, so there is nothing to filter.
- **User edits** are `TracksAction`s: `ResizeTrack`, `MoveClip`, `TrimClip` (the end may extend past the content; a clip's length is host data), `Select` and `Seek`. `TracksView::register(&view, path, window, cx)` puts the view on the action bus; pointer and keyboard input then dispatch the same erased action a bus client sends (`resize_track`, `move_clip`, `trim_clip`, `select`, `seek`), tagged with its `Source`. A drag previews locally and commits one action on release as a `dispatch_batch`, so it is one undo step. Each applied action emits `TracksEvent` (`ClipChanged { edit: Move | Trim(edge) }`, `TrackResized`, `SelectionChanged`, `Seek`) and returns its `TracksChange` as the bus outcome. `perform(action, cx)` applies one directly.
- **Rows** have their own heights (`TrackRow::with_height`, clamped to 20..=800 px). Layout is a prefix sum (`RowLayout`), and hit testing a binary search over row tops. `set_resizable_tracks(true, cx)` lets the user drag a row's bottom edge. A vertical wheel scrolls the rows when they overflow (Shift pans time); `scroll_y`/`set_scroll_y` read and set it.
- **Clip content:** `set_clip_painter(Some(Rc<dyn ClipPainter>), cx)`. `ClipPainter::paint(&ClipPaint, window, &App)` gets a snapshot of one visible clip (track, row, clip, on-screen bounds, the axis mapping time to x, selected) and only paints. It runs for visible clips of rows below overview density, after the clip background and before the selection.
- **Selection.** Click a clip to select it alone. Shift-, Ctrl-, or Command-click toggles it. Drag empty space to select the intersecting clips across rows, holding a modifier to add them. A right-click on a selected clip keeps the set. Every selected clip receives the theme's selection border and tint, or the host's `set_selection_style` override.
- Read stable keys with `selected_clips(cx)` and replace them silently with `select_many(keys, cx)` (store event `SetSelectionMany`). The first valid key is the primary clip used for keyboard edits. User changes emit `TracksEvent::SelectionChanged { selected, clips, source }`. The bus accepts a list of `{track, clip}` records on `select` alongside its single-clip and null forms.
- **Header column:** `set_header(HeaderMode::Default | Hidden | Custom(width, renderer), cx)`. A custom renderer gets a `HeaderRow` (index, row, height, selected) per visible row and is sized and aligned to it.

In a DataUi document, each track record may carry an `id` (its key; else its position) and a `height`, and the `header` prop is `default` or `hidden`.

Clip locks (`set_locked(ClipRef, ..)`) are keyed, so they stay with a clip when its track moves. The `clip-change` payload carries `trackId` (the key) as well as `track` (the display position).

Not yet: a clip-content pointer handler (first refusal on pointer events inside a clip). Painters only paint.

## DopeSheet authoring

The document is store state. `DopeDoc` holds `DopeChannel`s keyed by a host `ChannelId`, each with a `ChannelCurve` of `DopeKey`s keyed by `KeyId`, plus the selection (`KeyRef { channel, key }`) and the playhead. A channel is `ChannelKind::Keys` (markers), `Value` (keys carry values within `ValueBounds`; each segment interpolates by its first key's `Interp`: Constant, Linear, Bezier, EaseIn, EaseOut, EaseInOut) or `Toggle` (keys switch on or off). `dope_store(doc, undo, cx)` makes the `Store`, with undo over channel content only. `DopeSheet::with_store(store, format, length, cx)` renders it; `DopeSheet::new(channels, ..)` converts legacy `Channel`s (keyframe channels become value channels).

- **Host writes** are store events (`SetChannels`, diffed by id with view flags kept; `SetPlayhead`; `SetDopeSelection`). They never reach the bus and never emit a `DopeEvent`.
- **User edits** are `DopeAction`s: `InsertKey`, `RemoveKeys`, `RetimeKeys`, `DuplicateKeys`, `SetKeyValue`, `SetSegmentInterp`, `BendSegment`, `ToggleSpan`, `SetCurve`, `Select`, `SetChannelFlag` and `Seek`, with erased names `insert_key`, `remove_keys`, `retime_keys` and so on. `DopeSheet::register(&sheet, path, window, cx)` puts the sheet on the bus; pointer and keyboard input then dispatch the same erased action. Drags (retime, Alt-drag value, Ctrl-drag bend, a curve-lane edit) preview locally and commit one action in a `dispatch_batch`, so each is one undo step. Every applied action emits one `DopeEvent` and returns its `DopeChange` as the bus outcome; `perform(action, cx)` applies one directly.
- **Pointer:** double-click an empty cell to insert a key (keeping the curve's value); click a toggle row to flip it from the snapped time to the next switch; Alt-drag a value key vertically; Ctrl-drag inside a value segment to bend it; right-click a segment for the next interpolation.
- **Keyboard:** Up/Down move the row cursor, `[`/`]` select the previous/next key on it, `,`/`.` step the playhead a frame, K or Enter inserts at the playhead (toggles a toggle channel), I cycles the selected segments' interpolation, Left/Right nudge, Delete removes, Ctrl+A selects all, Ctrl+D duplicates.
- **Drawing:** value segments draw a mini-curve scaled to the row (a hold line over a light fill for Constant); toggle channels draw on-bars. Both use quads only. `set_show_segments(false, cx)` hides them.
- **`ChannelCurve::value_at(t)`** evaluates exactly the segment the sheet draws (`curves::segment_value`); it is pure, for sampling off the UI thread. `is_on(t)` reads a toggle channel.
- **Curve lanes:** `set_expanded(channel, true, cx)` (or the header button) opens an embedded `CurveView` under a value channel, synced from the store; its edits commit one `SetCurve`, which keeps key ids by time, then by position.
- **Formatting:** each channel has a `NumberFormat` (decimals, scale, unit), plain data. `set_value_formatter(Some(Rc<dyn ValueFormatter>), cx)` overrides it for the drag readout.

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

In a Rust host, subscribe to `TracksEvent::ContextMenu { track, clip, position, source }` to open a
host menu. A secondary click on an unselected clip selects it alone. Clicking a selected clip keeps the set. The menu event supplies the pointer position
in window coordinates. Empty space leaves selection alone. In a focused TracksView,
the Menu key and Shift+F10 use the selected clip's center, clamped to the visible
tracks body when scrolled out of view. With no selection these keys do nothing.
