# Production notes

## Recalculation mailboxes

An empty mailbox takes ownership of the first submitted vector in constant time.
Later edits retain one latest overlay value per distinct touched cell. The first
batch stays intact until the worker takes it, and the worker skips base inputs
covered by the overlay. These entries follow user edits. The worker can also own
one batch while applying it. UI submission proceeds without waiting for
calculation or channel capacity. Channels carry at most one wakeup each, and the output slot
holds at most one unread calculation. A completed calculation can share its
immutable values with the worker and with consumers retaining a snapshot.

`a_running_formula_is_superseded_off_thread` pauses a formula and checks that
10,000 superseded inputs leave no old payload references.
`unread_calculation_churn_retains_latest_publication_only` publishes 1,001
revisions while leaving the output unread.

`initial_submit_allocations_are_independent_of_batch_size` compares 1,000 and
100,000 entries using calling-thread allocation counts and bytes.
