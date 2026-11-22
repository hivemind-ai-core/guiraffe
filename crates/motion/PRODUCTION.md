# Production notes

## Layout presence

Presence entries follow current keys and exits still in progress. When an exit
settles and capacity exceeds the greater of 64 entries or four times the
remaining live count, the buffer shrinks to twice the remaining count with a
64-entry floor. This work is triggered only by a frame that drops entries.
See `presence_churn_releases_settled_exit_capacity`.
