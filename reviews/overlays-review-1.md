# Overlays review #1: MERGED with follow-ups

Must fix before 0.1:
1. A toast can get stuck invisible or hold its slot. The cap uses notification.max_items, but the native list counts unmanaged notifications too. With System delivery, a toast is never stored and is re-pushed forever. Force InApp delivery; drop and log if no entity is found; handle shared lists.
4. Confirm OK resolves stack.top(), not its own id (host.rs ~204). Resolve by id. Test stacked confirms.
Follow-ups:
2. Updating a toast re-pushes it, moving it to the end and replaying the enter animation. Update the entity in place.
3. An update while hovered drops the pause. Keep the hover state.
5. Removing or closing a dialog not on top leaves ghost stack entries; close() may close a non-managed dialog. Remove by id from any position.
6. The timer wakes 30 Hz while all toasts are paused. Count only unpaused auto-close toasts as active.
7. clean() doesn't notify.
8. position::position is unused by real widgets, and shift clamps the main axis (covering the anchor). Wire it in; cross-axis only by default.
9. The fuzzy matcher picks the first match, not the best. Use DP (fzf v2 style).
10. Spotlight re-lowercases everything per keystroke and searches twice. Precompute; bench 10k.
11. Desktop show() blocks. Add show_async on the background executor.
12. A ContextBuilder capturing strong Root/host makes a cycle. Clear on window close; document WeakEntity.
13. LoadingOverlay doesn't block the keyboard.
Tests: 3+ stacked mixed-policy layers, stacked confirms, toast update while hovered and its order, unmanaged plus managed toasts, System delivery, fuzzy 10k, tooltip positioning in the UI.
