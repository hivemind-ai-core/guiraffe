# Production notes

## Event and snapshot retention

Ingress coalesces scroll deltas by axis between ordered events. Each uninterrupted
burst uses at most one slot per axis. Accumulation preserves the total within
range and clamps to the public delta type at delivery. Activations, menu events,
and host transitions remain ordered and lossless. That FIFO is pending user
input and can grow while the UI is unable to drain it. Removal or receiver
teardown releases the backlog and closes ingress. See
`scroll_churn_coalesces_and_activation_order_survives_until_teardown`.

The SNI backend owns one flush at a time, one in-flight update snapshot, and one
replaceable pending snapshot. The flush checks the slot and relinquishes
ownership under the same lock. Connection-time updates use the same slot and
shutdown clears it. See `sni_updates_keep_one_flush_owner_and_the_final_snapshot`.
