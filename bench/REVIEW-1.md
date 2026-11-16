# Foreman review #1: bench (merge after the must-fixes)
MUST FIX before merge:
1. Ctrl-C leaves temporary worktrees (.git/worktrees/checkout*, /tmp dirs). Add a ctrlc handler (flag checked between samples, kill or wait for child processes), run `git worktree prune` plus reaping of prefixed leftovers on startup, and use a recognisable worktree name prefix.
3. Exit codes: 0 pass, 1 regression, 2 tool error. Test them.
SHOULD FIX (also in this pass, all small):
4. Use a dedicated target dir (target/bench-compare). Locate the binary via --message-format=json.
5. Warn when a ref resolves to HEAD while the tree is dirty, or offer a WORKTREE pseudo-ref. Compute `dirty` at the root.
8. Probe the old ref for bench support up front, with a clear message.
9. Compute timings in f64 and enforce a minimum measured duration per sample instead of rejecting 0.
10. Allocator: document the bytes semantics or count realloc deltas, and offer a process-global mode for gates.
11. Report "improvement" when the CI lies entirely below -threshold.
12. Don't commit loaded-host baselines as reference. Move them to bench/baselines/README notes or delete them.
FOLLOW-UP (record in VALIDATION.md, not now): 2 (allocation/render-count gates), 6 (prove the instruction gate fails red), 7 (persist nightly history beyond the Actions cache).
