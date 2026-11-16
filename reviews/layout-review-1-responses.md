# Responses to layout review 1

Branch: `feat-layout` (worktree `worktrees/feat-layout`). Every item was
answered with a test written first; the gate is `cargo fmt`,
`cargo clippy -p guiraffe-layout --all-targets -- -D warnings` and
`cargo test -p guiraffe-layout`.

## Item status

| # | Item | Status | What changed | Test |
|---|------|--------|--------------|------|
| 1 | HIGH `grid.rs placement()` overflows the grid | done | `Grid::layout()` now computes the rows and the grid lines itself. A column that does not fit in the row it would land in moves down to the next row; an offset is trimmed so the column still fits; the maths runs in `u32` and is clamped on the way into the `i16` the style needs. | `a_column_that_does_not_fit_moves_to_the_next_row` (unit + integration), `an_offset_that_does_not_fit_moves_the_column_to_the_next_row`, `a_column_never_reaches_past_the_last_column_of_the_grid`, `huge_numbers_do_not_overflow` |
| 2 | HIGH a column's default span is not the grid's width | done | The default span is `Span::Full`; it is resolved against the grid's column count when the grid renders. `Col::span` now takes a `Responsive<Span>`, so `Col::span(6)` and `Col::span(Span::Full)` both work. | `a_column_without_a_span_fills_the_grid`, `a_column_without_a_span_fills_a_wider_grid_too`, `a_column_without_a_span_fills_the_grid` (integration) |
| 3 | HIGH responsive props follow the window, not the container | done | `ContainerWidth::Measured(Entity<ContainerWidthState>)` measures the box the widget is laid out in (the prepaint bounds of the widget's own box) and keeps it in a small entity that only notifies when the *band* changes. A grid inside a narrow pane therefore answers to the pane. | `a_grid_follows_the_container_it_is_in_and_not_the_window` (integration) |
| 4 | MED-HIGH no band below `Xs` | done | `Breakpoint::Base` is the band below 576px; `Breakpoint::ALL` starts with it and `min_width(Base)` is 0. | `a_width_below_the_smallest_band_is_in_the_base_band`, `an_xs_override_only_starts_at_the_xs_band` (at 0, 575 and 576), `the_base_band_can_be_set_on_its_own` |
| 5 | MED the splitter is thin | done | `.persist_key()` keeps the sizes in the application's `SplitterStore` and takes them again when the splitter is built; `SplitterSizes` is serde. `.max_size()`, a theme default minimum, any number of panes (`.pane()`), and a debug assert for a minimum wider than the maximum. | `the_sizes_can_be_stored_as_json`, `a_splitter_can_hold_any_number_of_panes`, `a_pane_that_is_narrower_than_it_is_wide_is_noticed`, `a_new_splitter_is_a_horizontal_split_with_two_empty_panes`, `a_splitter_gives_every_pane_its_room`, `a_pane_does_not_become_narrower_than_its_minimum` |
| 6 | MED render counters around the splitter | partly | A drag that leaves the panes where they were is not reported: the sizes at the end of the drag are compared with the sizes the drag started from. The buffer the sizes are copied into is reused, so a drag does not allocate a vector per event. **Left open:** the state of the resizable group that comes with `gpui-kit` says something on every mouse move of a drag, which renders the view that holds the splitter again — a drag that changes nothing still renders it. Fixing that needs a resizable group of our own instead of the one `gpui-kit` ships; the rest of the item (the report, the allocation and the counters) is in place. | `a_drag_that_changes_nothing_reports_no_resize_and_leaves_the_siblings_alone` |
| 7 | MED the shell is not a control | partly | `AppShellState` is an entity the application owns: `AppShell::new(&state)`. A collapsed navbar keeps its place (width 0, still in the tree), so what is inside it keeps its state. **Left open:** "collapse must not re-render main". The shell reads the state while it renders, so the view that holds the shell is rendered again when the state changes, and in this version of GPUI that renders the entity views inside it again. Making the main immune to it needs the shell to read the state inside a view of its own, which in turn needs the parts of the shell to be views rather than elements. | `a_collapsed_navbar_takes_no_room_but_stays_where_it_is`, `a_button_in_the_navbar_can_collapse_it`, `the_navbar_can_be_collapsed_and_opened`, `the_aside_can_be_collapsed` |
| 8 | MED the shell is rigid | done | The width of the navbar and of the aside live in the state and can be changed through it, the aside can be collapsed, and the navbar collapses on a window narrower than the width the state was given. The bars take their width from the size scale, and the tests do not name a pixel width of their own. | `a_narrow_window_collapses_the_navbar`, `the_bars_can_be_resized`, `the_bars_have_default_sizes`, `an_app_shell_keeps_its_parts_where_they_belong` |
| 9 | MED the container has a fixed width | done | The container is full width with a maximum width, so it fits a narrow parent instead of reaching past it. | `a_container_fits_the_space_it_is_given`, `a_container_is_centered_and_never_wider_than_its_size` |
| 10 | MED `grow` does nothing | done | A growing group shares the width out between its children; a growing grid gives the room that is left to its last row. | `a_growing_group_spreads_its_children_over_the_width`, `a_growing_grid_fills_the_room_with_its_last_row` |
| 11 | MED raw colours and sizes in the widgets | done | The divider takes its width, its colours and the size of its text from the theme; a gap is never negative and never a number that is not a number, and a developer who asks for one is told about it. | `a_new_divider_is_a_horizontal_rule_without_a_label`, `a_gap_keeps_its_value`, `a_negative_gap_is_noticed_in_a_debug_build`, `a_gap_that_is_not_a_number_is_noticed_in_a_debug_build` |
| 12 | LOW the aspect ratio takes anything | done | The ratio is limited to `0.01..=100`; a ratio that is not a number falls back to a square. The box is positioned and clips what does not fit. | `a_ratio_stays_inside_the_range_a_box_can_hold`, `a_ratio_that_is_not_a_number_falls_back_to_a_square` |
| 13 | LOW the simple grid is fixed | done | The columns, the spacing and the vertical spacing take responsive values. | `the_columns_can_change_with_the_width`, `the_spacing_can_change_with_the_width`, `the_vertical_spacing_can_change_with_the_width` |

## Commits

| Commit | Item |
|--------|------|
| `2e487e9` | #4 the base band |
| `9d01270` | #1, #2 the grid lays its own columns out |
| `52f9f0b` | #9 the container fills the space it is given |
| `fc23e4f` | #12 the aspect ratio in range |
| `b5073fc` | #13 the simple grid answers to the width |
| `a59ffad` | #11 the divider and the gaps take the theme |
| `6792aa3` | #5, #6 the splitter's keys, limits and panes |
| `8a009ed` | #3, #7, #8 the shell as a control, the measured container |

## Test counts

`cargo test -p guiraffe-layout`:

| Suite | Before | After |
|-------|--------|-------|
| unit (`src`) | 63 | 88 |
| integration (`tests/layout.rs`) | 16 | 22 |
| interaction (`tests/interaction.rs`) | 3 | 8 |
| doc | 17 | 19 |
| **total** | **99** | **137** |

`cargo fmt --all -- --check`, `cargo clippy -p guiraffe-layout --all-targets
-- -D warnings` and `cargo test -p guiraffe-layout` are green.
