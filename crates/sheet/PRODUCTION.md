# Production notes

## Recalculation mailboxes

Pending edits retain the newest input per distinct cell touched since the worker
last took the mailbox. This count follows the user's edits. The worker can also
own one batch while applying it. The UI submits without waiting for calculation
or channel capacity. Channels carry at most one wakeup each, and the output slot
holds at most one unread calculation. A completed calculation can share its
immutable values with the worker and with consumers retaining a snapshot.

`a_running_formula_is_superseded_off_thread` pauses a formula and checks that
10,000 superseded inputs leave no old payload references.
`unread_calculation_churn_retains_latest_publication_only` publishes 1,001
revisions while leaving the output unread.
