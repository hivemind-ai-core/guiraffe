# Production notes

## Delayed actions

The store retains one task and payload per live debounce key. Superseding a key
replaces the record and drops its captured action immediately. A timer takes
the action only if its ticket still matches. Store teardown releases the keyed
records. Unkeyed delayed actions remain independently scheduled until delivery
or executor teardown. See `debounce_churn_releases_superseded_actions_immediately`.
