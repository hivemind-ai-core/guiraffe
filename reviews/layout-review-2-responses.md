# Responses to the layout review #2

Every item of `reviews/layout-review-2.md` is fixed. The branch is rebased on
`release/0.1` (which moved while the work was going, so it was rebased twice),
and the gate is green: `cargo fmt -p guiraffe-layout --check`,
`cargo clippy -p guiraffe-layout --all-targets -- -D warnings` and
`cargo test -p guiraffe-layout` (105 unit + 18 interaction + 25 layout + 24 doc
tests).

Every regression test below was checked by putting the fix back the way it was
and running the test again: the test fails without the fix. The revert that was
used is named after the test.

## The items of review #1 that were not finished

### 1. An offset after a sibling overlaps it

An offset is now the room a column leaves empty *after* the column before it,
the way Mantine counts it: the column starts at `cursor + offset + 1`. A column
that does not fit in the room its offset leaves moves down to the next row,
where the offset is trimmed to what that row can afford.

* `crates/layout/src/grid.rs`: `an_offset_counts_from_where_the_column_before_it_ends`,
  `an_offset_after_a_column_that_fills_the_row_moves_the_column_down`,
  `an_offset_that_does_not_fit_the_row_is_trimmed` (N9).
* `crates/layout/tests/layout.rs`: `an_offset_leaves_the_room_after_the_previous_column_empty`
  (the old test named the old meaning and was rewritten).

Revert checked: with the old `offset + 1` placement the unit test fails and the
integration test measures the column on top of its neighbour.

### 3. The measurement test proves nothing, and measured is not the default

The container in the test is now 400 pixels wide (narrower than the smallest
band) inside the default window (wider than it), so the two are in different
bands. A grid and a simple grid measure the box they are laid out in by
default, so a grid in a narrow pane answers to the pane and not to the window.
A grid that measures its container fills it, the way a grid of columns does.

* `crates/layout/tests/layout.rs`: `a_grid_follows_the_container_it_is_in_and_not_the_window`,
  `a_simple_grid_counts_its_cells_against_the_container_it_is_in`,
  `a_grid_in_a_pane_follows_the_pane_and_not_the_window`.
* `crates/layout/src/width.rs`: `a_measurement_that_has_not_happened_yet_is_the_width_of_the_window`
  (N8).

Revert checked: with the grid resolving against the viewport the first and the
third test fail (400px instead of 200px, and an empty column in the pane).

### 5. The debug assert says the wrong thing, and the sizes are not checked

The message now says what is wrong ("a pane cannot be wider than the narrowest
it may become"), and the check lives in a pure function, `size_range`, that is
tested on its own. The sizes a splitter takes back are checked before they are
used: a size that is not a number or not positive is not a size a pane can
have, a size for a pane that is not there is dropped, and the sizes that are
fit to use add up to the room the panes share.

* `crates/layout/src/splitter.rs`: `a_pane_that_is_narrower_than_it_is_wide_is_noticed`,
  `a_pane_may_be_as_wide_as_it_wants`, `the_sizes_that_are_stored_are_checked_before_they_are_used`,
  `sizes_for_another_number_of_panes_are_fitted_to_the_panes`,
  `the_sizes_are_scaled_to_the_total_the_panes_share` (N5).
* `crates/layout/tests/interaction.rs`:
  `a_splitter_that_restores_the_sizes_of_another_number_of_panes_uses_sizes_that_fit`.

Revert checked: with the old message the `should_panic` test fails, and with
the old "the stored sizes are used as they are" the restoring test fails.

### 6. The sibling render count is never asserted

The test now asserts the render count of the sibling, and the drag is a real
drag: the divider is pressed on its handle, moved past the point where a drag
begins and released where it was, so the panes keep the sizes they had. The
old test moved the pointer two pixels, which never starts a drag, so it never
reached the code it meant to test.

* `crates/layout/tests/interaction.rs`:
  `a_drag_that_changes_nothing_reports_no_resize_and_leaves_the_siblings_alone`.

Revert checked: with the comparison of the sizes taken out, the report fires
and the test fails.

### 7. The shell is not a control, and the main content is rendered again

The shell state is an `EventEmitter<AppShellEvent>` now, and the main content is
a view the shell keeps: the content is not built again when the shell collapses
a bar. The content is laid out again, because the room it has changes.

* `crates/layout/tests/interaction.rs`:
  `the_main_content_is_not_rendered_again_when_the_navbar_collapses`,
  `the_shell_tells_what_it_did` (N10).

Revert checked: with the main content put back as a plain element, the content
is built again on every frame the shell renders and the render count assertion
fails.

### 8. The shell is rigid

The navbar and the aside can be dragged to another width, the aside has a band
of its own that collapses it, and `collapse_below` names a band instead of a
number of pixels.

* `crates/layout/tests/interaction.rs`:
  `the_user_can_drag_the_navbar_to_another_width`, `a_narrow_window_collapses_the_aside`.
* `crates/layout/src/app_shell.rs`: `a_band_names_the_width_the_bars_collapse_below`.

Revert checked: without the handle the drag changes nothing and the width
assertion fails; without the band the aside keeps its room and the collapse
assertion fails.

### 12. A bad ratio is not noticed

`checked_ratio` tells the developer about a ratio that is not a number or one
outside the range a box can hold, and the box still keeps a ratio that works.

* `crates/layout/src/aspect_ratio.rs`:
  `a_ratio_outside_the_range_a_box_can_hold_is_noticed`,
  `a_ratio_that_is_not_a_number_is_noticed`.

Revert checked: without the assertions the two tests fail, because nothing
panics any more.

## The new defects

### N1. The measurement keeps the band, not the width

The measurement keeps the exact width it was given and only says something when
the band changes.

* `crates/layout/src/width.rs`: `a_measurement_keeps_the_exact_width_it_was_given`,
  `a_measurement_only_says_something_when_the_band_changes`.
* `crates/layout/tests/layout.rs`: `a_simple_grid_counts_its_cells_against_the_container_it_is_in`.

Revert checked: with the width only kept when the band changes, the simple grid
counts its cells against a width of zero and the test fails.

### N2. One measurement, two widgets, an endless loop

A measurement belongs to one widget. The first widget that measures owns it,
another one is refused, and the developer is told about it.

* `crates/layout/src/width.rs`: `a_measurement_belongs_to_one_widget`,
  `two_widgets_of_different_widths_cannot_flip_the_band_forever`,
  `a_measurement_from_another_widget_is_noticed`.
* `crates/layout/tests/layout.rs`: `a_measurement_that_is_shared_by_two_widgets_is_noticed`.

Revert checked: with the refusal taken out, the band flips between the two
widths for ever and the two unit tests fail.

### N3. A key alone keeps nothing

The splitter keeps its sizes when it has a key, with or without a callback.

* `crates/layout/tests/interaction.rs`:
  `a_splitter_with_a_key_keeps_its_sizes_even_without_a_callback`.

Revert checked: with the keeping of the sizes behind the callback again, the
store stays empty and the test fails.

### N4. The buffer of the report is new every time

The buffer the sizes are compared with lives in the state, so it survives from
one render to the next. A drag that changes nothing is not reported and does
not write to the store.

* `crates/layout/tests/interaction.rs`: `a_drag_reports_the_new_sizes_once`,
  `a_drag_that_changes_nothing_does_not_write_to_the_store`.

Revert checked: with the comparison taken out, the report fires and the store
is written, and both tests fail.

### N5. The sizes that are stored are used as they are

The sizes that are stored are checked before they are used, and the sizes that
are fit to use add up to the room the panes share.

* `crates/layout/src/splitter.rs`: `the_sizes_that_are_stored_are_checked_before_they_are_used`,
  `sizes_for_another_number_of_panes_are_fitted_to_the_panes`,
  `the_sizes_are_scaled_to_the_total_the_panes_share`.
* `crates/layout/tests/interaction.rs`:
  `a_splitter_that_restores_the_sizes_of_another_number_of_panes_uses_sizes_that_fit`.

### N6. A store behind a mutex

The layout crate asks a store for the values with the `LayoutPersist` trait and
ships an implementation that keeps them in memory, without a mutex: the
application is single-threaded. The core crate can put a store of its own
behind the trait.

* `crates/layout/src/splitter.rs`: `a_store_keeps_the_values_of_a_key`,
  `an_application_without_a_store_has_none`, `a_store_can_be_installed_by_the_application`,
  `a_store_of_your_own_can_be_used`.

### N7. The shell undoes the toggle of the user at every resize

A bar collapses when the width crosses the band it was given, and not at every
resize: a toggle the user made survives a resize that stays in the same band.

* `crates/layout/tests/interaction.rs`: `a_toggle_the_user_made_is_not_undone_by_a_resize`.

Revert checked: with the bar collapsed on every measurement, the resize undoes
the toggle and the test fails.

### N8. The first frame is stale

A measurement lands after the box has been painted, so the width of the window
is the starting point: a widget answers to something sensible from the first
frame and the measurement takes over on the next one. What is left is the frame
in between: a container that changes its width between two frames is answered
to a frame late. This is written down in the module documentation of
`crates/layout/src/width.rs`.

* `crates/layout/src/width.rs`: `a_measurement_that_has_not_happened_yet_is_the_width_of_the_window`.

### N9. An offset that does not fit the row

A column that does not fit in the room its offset leaves moves down to the next
row, where the offset is trimmed to what that row can afford.

* `crates/layout/src/grid.rs`: `an_offset_that_does_not_fit_the_row_is_trimmed`.

### N10. The widgets do not say what they did

The splitter state emits `SplitterEvent::Resized` and the shell state emits
`AppShellEvent` (`NavbarCollapsed`, `AsideCollapsed`, `NavbarResized`,
`AsideResized`).

* `crates/layout/tests/interaction.rs`: `a_drag_reports_the_new_sizes_once`,
  `the_shell_tells_what_it_did`.

## What changed in the public API

* `ContainerWidthState` keeps the exact width and knows the widget that owns
  the measurement.
* `Grid` and `SimpleGrid` measure the box they are laid out in by default;
  `container_width` names a width of your own.
* `SplitterState` is an entity of the layout crate now (`SplitterState::new`),
  and it emits `SplitterEvent`.
* `SplitterStore` and `splitter_store` are gone: `LayoutStore`, `LayoutPersist`,
  `MemoryPersist` and `layout_store` take their place.
* `AppShellState` emits `AppShellEvent`; `collapse_below` and
  `collapse_aside_below` name a band, and the bars can be dragged.
* `AppShell::main` takes a view, so the shell can keep it.
