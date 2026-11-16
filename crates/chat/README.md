# guiraffe-chat

A window-local agent conversation control for GUIraffe / GPUI Kit 0.9.1.

```rust,ignore
guiraffe_chat::init(cx);
let chat = cx.new(|cx| ChatState::new(window, cx));
// Retain `chat` in your view and render Chat::new(&chat).
```

Subscribe to `ChatEvent::Submitted` and connect your provider. Call
`ChatState::begin_reply(cx)` to obtain a message id and a non-clonable `Producer`.
Move the producer to a background task and push owned token chunks. Dropping it
finishes the response. `PushError::Full(chunk)` returns ownership for retry;
`Closed` means the conversation stopped, was replaced, or was dropped. Never spin
on a full queue on the UI thread. The example and gallery show asynchronous retry.

The channel holds 64 chunks per reply, each at most 64 KiB; a foreground tick
consumes at most eight chunks. It uses Crossbeam's lock-free bounded queue, with
atomic cancellation and completion publication. No UI mutex or document-wide
clone occurs per token. `benches/handoff.rs` measures the hand-off independently
of rendering. Property tests compare delivery against a sequential model, and a
cross-thread stress test checks ordering and completion at four capacities.

History uses GPUI's native variable-height `ListState`, following the tail until
a wheel scroll moves away. New content keeps the user's anchor; the jump button
resumes following. Each message retains its native `TextViewState`, parsing
streamed Markdown through `push_str`; selection and code-copy controls survive
virtualization. Message entities, composer and history have separate lifetimes.
Tool cards show running/succeeded/failed states and expandable arguments/results;
reasoning starts collapsed and can be replaced with `set_reasoning`. New-message Presence uses the application's reduced
motion preference and stops requesting frames when settled.

The composer uses native multiline input: Enter sends, Shift+Enter inserts a
newline. Slash and mention completion use the overlays crate's prepared fuzzy
index. Arrow keys select, Enter/Tab accept and Escape dismisses the popup under
`GrChatCompletion > Input`. The default catalogs can be replaced through
`Composer::set_completions`. Attachment buttons emit `AttachmentRequested`;
applications supply real file dialogs and add chips with `add_attachment`.
Copy acts locally; retry emits an id for the host. Editing the last user message
replaces that message and removes the subsequent branch when submitted; Cancel
leaves history intact. Display timestamps are supplied by the host.

`ChatProps` is serializable/schema-backed. Theme colors, surfaces and intrinsic
spacing come from `GuiraffeTheme` and the inherited `StyleContext`. `Chat` is
`Styled`, with no outer placement or spacing imposed on its caller.

## Retained rendering and the native scroll-layer limitation

The gpui-fast tests distinguish two rendering paths. In the steady-layout fixture,
with composited scroll layers disabled, streaming rebuilds only the active message: conversation parent,
composer and earlier message render counts remain unchanged. The test explicitly
selects that native backend policy; it does not silently change library settings.

With gpui-fast 0.1.15's default scroll layers, the native list may repaint its
visible rows when streamed content changes. That backend deliberately bypasses
nested view reuse while recording a scroll layer. The parent and composer remain
retained and offscreen messages stay virtualized, but zero earlier-row renders is
not guaranteed in this mode. An application requiring the strict view-retention
path can launch with `GPUI_SCROLL_LAYERS=0`; this widget never sets or reads an
environment variable itself. Both paths have interaction/render-count coverage.

Appearance snapshots use four fully redrawn headless frames with bundled assets;
render isolation is measured separately with ordinary retained frames. Snapshot
comparison and per-backend goldens use `guiraffe-test::snapshot`. The local driver
avoids a cached-header repaint artifact observed with the shared helper's initial
two-frame capture. Gallery snapshots also exercise the complete shell/window root;
its existing helper currently omits its configured SVG assets.

Run `cargo run -p guiraffe-chat --example agent_chat --features gpui-fast` for the
standalone simulated provider, or select **Agent chat** in the gallery. Neither
example sends network requests.
