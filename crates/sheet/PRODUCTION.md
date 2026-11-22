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


### Release-scale Sheet probe

`cargo test --release -p guiraffe-sheet --test submit_allocations
sheet_submission_and_mount_probe -- --ignored --nocapture` measures
100,000 chained formulas in a sparse 1,000,000 × 1,000 sheet at 1,280 × 760.
Submit samples time the prepared batch handoff. Mount samples include fixture
model construction and the first drawn frame. Five fresh processes per revision
ran on a shared AMD Ryzen 7 8745HS host with the upstream test backend.

| Revision | Median submit | Cold submit allocations and bytes | Median mount |
|---|---:|---:|---:|
| `5fc20c8` | 16.156 ms | 6,246 allocations, 16,334,536 B | 53.726 ms |
| `896167f` | 16.831 µs | 4 allocations, 2,896 B | 41.667 ms |

The warm-thread regression failed at 61 versus 6,245 allocations on the
before build, and passed with equal counts and bytes on the after build.
