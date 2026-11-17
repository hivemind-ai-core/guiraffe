# Superseded initial measurements

These records belong to pre-review source revision 3f1ee1d. They are retained for audit,
not as evidence for the current implementation or performance acceptance.

The old `code-snapshot-handoff` workload performed 1,024 Arc clone/drop pairs, not
worker delivery or UI publication. It has been renamed `code-snapshot-arc-clone`.
The old model benchmark records contain ten aggregate samples with an iterations
field of 100; those counts must not be read as 100 retained raw samples. The headless
draw record has summaries only and no retained per-frame samples. Current validation
makes no timing, speed-up, presentation-rate or cross-thread publication-cost claim
from any of these records.
