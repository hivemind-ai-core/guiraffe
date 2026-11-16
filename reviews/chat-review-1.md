# Chat review #1: merged with follow-ups (owner: chat author)
1. Throughput: drain by a byte budget per frame, add an async-wait or "space available" producer API, and document backpressure.
2. Event-driven wake-ups: no 60 Hz timer while there are no tokens.
3. Strengthen the isolation test: earlier messages and History re-render 0 extra times on token frames; run 50+ frames.
4. §6.2 notify-as-write violations: lib.rs 419, 481, 538, 579, 595; message.rs 139.
5. Remove the double remeasure per batch, and the duplicate text storage in Message and TextViewState.
6. Stop: drain the queue first, mark the message stopped, and emit Stopped when begin_reply cancels a stream.
7. Retry only on the last assistant message and only when not streaming; add a truncate_after/retry helper.
8. Replace panicking indexing with get().
9. Composer: IME-aware Enter, and a dismissed popup should stay dismissed.
10. A "streaming below" indicator on jump-to-latest.
11. Visual: left-align the disclosure rows; disabled Send in dark mode; attachment chip styling; consistent avatar shape.
12. Tests: empty conversation, more than 64 KB, edit or retry while streaming, IME, a token-flood frame benchmark.
