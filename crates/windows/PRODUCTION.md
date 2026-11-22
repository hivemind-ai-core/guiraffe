# Production notes

## Notification and layout retention

The notification service admits every pending card in FIFO order. The visible
limit bounds only displayed cards. Pending cards and their handlers follow
outstanding requests. Dismiss releases queued content and sends its terminal
event, which releases the handler after delivery. Clear discards all pending
content in one pass, then starts visible exits. Service teardown drops its
remaining content and handlers. See
`overflow_notifications_release_callbacks_and_payloads_on_dismiss_and_clear`
and `notification_owner_teardown_releases_queued_handlers`.

Saved layouts are app data. Closing a window retains its saved record.
`WindowManager::forget_saved_layout` removes one record and writes the remaining
layouts when persistence is configured. An open window may record bounds again.
`forgotten_window_layouts_do_not_return_after_reload` verifies deletion across
reload while a distinct layout remains saved.
