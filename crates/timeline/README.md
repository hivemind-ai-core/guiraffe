# guiraffe-timeline

Time-based plots and editors on `PlotSurface`, with pure axis, curve, channel-selection, clip and schedule models. Initialize Kit with `gpui_kit::component::init(cx)` and apply a `GuiraffeTheme` before constructing native views.

`PlotSurface` has an x `Axis`, optional y `ValueRange` and ordered `Layer` stack. Custom layers append visible-coordinate `plot::Primitive` commands in `Layer::prepaint`; the native canvas paints them in order. Built-ins are `Ruler`, `Gridlines`, `GridMarks`, `Playhead`, `SelectionBox`, `Markers`, `Zebra` and `BackgroundBands`. `add_layer`, `insert_layer` and `clear_layers` control composition. `navigate` implements pan/anchored zoom, and `fit` frames a range. Ruler layers target a separate header, sharing the body axis.

All editor views accept a layer with one call:

```rust,ignore
view.update(cx, |view, cx| view.add_layer(Gridlines::default(), cx));
```

Gantt and Tracks retain their scheduling/clip APIs. Global `set_editable(false, cx)` prevents gestures and leaves item appearance alone. Item locks remain independent; label-column Kit icon buttons toggle them. `set_lock_style(item, global, cx)` configures `None`, `Badge`, `Dimmed` or `Hatched`; defaults are an item badge and no global styling. Lock styling never changes edit admission.

Dope sheets accept `Channel::group(name, children)` and `Channel::with_keyframes(name, keys)` as well as time-only channels. Group rows aggregate descendant times; channel controls expand/collapse, mute, lock and hide keys. Only visible rows and horizontally visible key glyphs instantiate/paint. Click selects, Shift-click toggles selection, dragging moves selected editable keys together with frame snapping, and dragging empty space selects a box. Collisions reject the entire move. `set_focusable(true, cx)` opts into pointer focus and keyboard editing: Ctrl/Command+A selects all, Ctrl/Command+D duplicates one frame later, arrows nudge, and Delete/Backspace removes selected editable keys. Group collapse retains selection. Mute/visibility are host-readable channel state; this crate does not evaluate animation playback.

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

See `examples/plot_layers.rs` and the five showcase timeline entries. `VALIDATION-2.md` records acceptance evidence and the native-input rendering boundary.
