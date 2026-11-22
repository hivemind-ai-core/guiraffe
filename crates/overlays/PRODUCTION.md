# Production notes

## Toast requests and context registrations

The toast queue retains one record per outstanding toast ID, with up to `limit` visible.
The unshown FIFO backlog follows caller demand. Promotion, expiry of visible toasts,
explicit removal and clearing service the backlog. Covered by
`backlog_tracks_pending_requests_through_churn`.

A window holds one context builder per registered name. Re-registering replaces the
builder, `OverlayHost::remove_context` releases it, and closing the window clears the
registry. Covered by `removed_contexts_release_builders_under_churn`.

Dismissal reduces substantially oversized queue storage. Clearing toasts drops the
overflow buffer, and window teardown releases the queue. The visible limit does
not cap pending app data. `overflow_toasts_survive_admission_and_release_on_dismiss_or_clear`
checks that 2,000 pending toasts survive admission and that dismissal and clear
reclaim their storage.
`window_close_releases_toasts_even_when_host_handle_survives` covers teardown.
Context registry capacity can reflect peak registrations.
