# Production notes

## In-memory splitter settings

`MemoryPersist` owns one saved value per distinct key until the key is overwritten,
explicitly forgotten with `remove`, or the store drops. Its size follows the caller’s
saved keys and their values. Use `remove` for permanently retired keys. Covered by
`retired_keys_release_saved_values_under_churn`.

The map can reuse its bucket capacity from the peak saved-key count.
