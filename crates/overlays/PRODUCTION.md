# Production notes

## Toast requests and context registrations

The toast queue retains one record per outstanding toast ID, with up to `limit` visible.
The unshown FIFO backlog follows caller demand. Promotion, expiry of visible toasts,
explicit removal and clearing service the backlog. Covered by
`backlog_tracks_pending_requests_through_churn`.

A window holds one context builder per registered name. Re-registering replaces the
builder, `OverlayHost::remove_context` releases it, and closing the window clears the
registry. Covered by `removed_contexts_release_builders_under_churn`.

Queue and map capacity can reflect peak backlog and registration counts.
