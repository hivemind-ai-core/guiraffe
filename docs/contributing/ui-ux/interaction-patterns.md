# Interaction patterns

| Pattern | Design decision | Verify |
|---|---|---|
| Navigation | Name destinations after the user's work. Show current location and preserve meaningful view state when returning. | Back restores the expected view, selection and scroll position where appropriate. |
| Commands | Keep frequent actions discoverable in context. Show platform-appropriate shortcuts in menus or help and expose less frequent actions through search or overflow. | Mouse, keyboard and menu routes have consistent enablement and effects. Text editing retains its shortcuts. |
| Forms | Use persistent labels, explain format before entry and preserve input across failures. Validate at a useful boundary. Link errors to fields and give a recovery action. | A user can correct multiple errors and resubmit without redoing valid work. |
| Destructive actions | Use undo for safely reversible changes. For irreversible actions, describe what will be affected and name the commitment. | Cancel preserves state, undo restores it, and partial failures identify affected items. |
| Search and filtering | Keep query and active filters visible. Distinguish a first-use collection from zero results and provide an appropriate next action. | Clearing filters restores results without clearing unrelated preferences. |
| Lists and tables | Give selection a stable identity through sort and refresh. Separate row activation from row actions. Keep headers, sort state and units understandable. | Bulk actions state their scope across filtered or paged data. Keyboard navigation remains useful with virtualization. |
| Overlays | Use transient UI for a local action, and a modal for decisions that need exclusive attention. Restore focus to the opener or a meaningful fallback. | Tab behavior, Escape, cancellation and menus nested inside a dialog behave predictably. |
| Drag and canvas editing | Show the candidate result, allow cancel, and supply a keyboard or single-pointer action for the same outcome. | Move, resize or link a representative object without a drag. |
| Settings | Explain effects in the user's vocabulary. Clarify immediate versus committed changes and offer a reset at an understandable scope. | Closing with unsaved changes has predictable consequences. |
| First use and help | Put the next useful step in the interface. Offer examples and contextual help where decisions are difficult. | A first-time user can start and an experienced user can skip instruction. |

## State and recovery

Define ready, first use, loading, refreshing, empty search, blocked permission, partial success, failure and success wherever they apply. Preserve the previous result when it is useful and mark it as stale. Use determinate progress only when the system can estimate it meaningfully. Announce meaningful changes through the appropriate accessibility mechanism. Preserve focus unless completing the action requires moving it. Show an inline message for a problem tied to a field or region, and keep important unresolved problems visible after temporary feedback expires.

Write errors in three parts as needed: what failed, the consequence for the user's work, and the next available action. Keep retry safe from duplicate effects. For example, “Export failed. Your project is saved. Choose a writable folder and try again.” Test interrupted and repeated actions in addition to the happy path.

Sources: [Nielsen’s usability heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/), [GNOME patterns](https://developer.gnome.org/hig/), [WAI keyboard guidance](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/).
