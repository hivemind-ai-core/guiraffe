# Production notes

## Store registrations

The panel holds one adapter reference per registration. `Devtools::remove_store` removes
every reference to the given adapter in this panel and clears its recording callback.
The application owns the registration lifetime and should disconnect the adapter when
done. Covered by `disconnected_stores_release_adapters_under_churn`.

The registry vector can reuse its capacity from the peak registration count.
