# Router review #1: merged after fixes
Fixed, with tests:
- #1: round_trips() check, plus a debug_assert on navigate. Empty path params are strict.
- #2/#9: Route::check() reports orphan params, typos, duplicates and a misplaced *rest. The router debug_asserts it.
- #3: GuardOutcome {Allow, Block, Redirect}. Navigating from inside a guard returns Blocked. Redirect loops are blocked.
- #5: '+' is literal in path segments and a space in queries.
- #6: back/forward are absolute, verified index moves.
- #7: search params are lenient (unparsable becomes the default).
- #8: *rest normalisation documented.
- History::from_parts/go_to for persistence.
Async confirm (#4): Block, open a dialog, then navigate_unguarded on "Discard" (documented pattern).
Follow-ups (the spec's next steps):
- Option<T> search params via a wrapper type.
- Nested layouts/Outlet plus loaders through guiraffe-query.
- A not-found fallback.
- Per-window integration with guiraffe-windows: a Router per window, history persisted with the layout, open_in_new_window via WindowKey.
- serde for History.
- A #[derive(Route)] proc-macro to replace routes!, if field attributes are needed.
