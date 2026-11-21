# CodeViewer annotation follow-up

The comparison starts at `7925639`. The follow-up isolates the blame age bar from the author, shows layered review details on hover in the analysis margin, colours diagnostic suffixes by severity, and adds a checkout fixture with independently switchable layers. The hosted review example uses a themed card with an initials avatar, an author, and a copy action.

| Measure | Before | After |
|---|---:|---:|
| Mounted replay setup and first frame | 59.3 ms | 170.7 ms on lower-load replay |
| Mounted replay p99, two ten-second windows | 2.05 / 2.35 ms | 2.87 / 6.77 ms on lower-load replay |
| Mounted replay maximum frame work | 3.28 ms | 8.16 ms on lower-load replay |
| Mounted replay steady RSS | 68,432 / 68,640 KiB | 68,896 / 68,960 KiB on lower-load replay |
| BoundsTree insertion, samply self | 13.08% | 12.07% |
| Text raster bounds, samply self | 12.74% | 9.36% |
| GPUI text paint layer, samply self | 12.06% | 6.65% |
| Decoration paint, samply self | 8.19% | 9.52% |
| Overlay paint, samply self | 7.85% | 7.87% |
| Packed interval construction plus one million viewport queries, Callgrind Ir | 800,144,527 | 800,144,517 |
| Warm cached annotated paint, eight upstream draws | 56 allocations / 74,368 bytes | 56 allocations / 74,368 bytes |
| Viewport query allocations at 10k and 1M records | 0 / 0 | 0 / 0 |

The pre-change timing and 1,200-frame profile are from the previous acceptance run. The post-change timing and 1,200-frame profile ran while other worktrees were building and testing. Two higher-load replays measured 96.1 ms mount and 7.50 / 8.74 ms p99, then 317.9 ms mount and 8.15 / 9.06 ms p99. The lower-load replay met the 200 ms mount and 8 ms p99 budgets while a showcase test was still running. The 59 ms and roughly 2.2 ms quiet-window targets remain unverified. Callgrind used the same standalone fixture compiled against each revision, with a matching checksum of 25,257,335,625. It measures interval work. The shared GPUI allocation follow-up remains with the foreman.

The new regression tests cover review hover entry and exit, and colour and source-position preservation for rendered suffixes. The layered golden explicitly opens a review hover card. The showcase golden uses the idle state. The active layered review cache holds at most one Markdown entity. Insertion colours use one entry per prepared inline insertion. Existing viewport block and shape bounds are documented in [PRODUCTION.md](../PRODUCTION.md).

The full `scripts/gate.sh` passed on `5fbd48a` with upstream and GPUI Fast tests, per-crate doctests, Clippy with warnings denied, and formatting. I regenerated and viewed all eight changed light and dark goldens across the two backends.
