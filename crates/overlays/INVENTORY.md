# Controls 0.7.0 inventory

Inspected the installed `gpui-component-0.7.0/src` sources before implementation.

| Primitive | Existing behavior | GUIraffe addition |
| --- | --- | --- |
| dialog/dialog.rs | stacked window dialogs, title/content/footer, confirm/cancel callbacks, Esc/outside policies, focus scope, animation | named context builders, IDs, awaitable confirmation, stack bookkeeping |
| sheet.rs | four placements via WindowExt, resizable sheet, overlay, focus scope and animation | Drawer with theme size and policy; one sheet per window, below dialogs |
| popover.rs | controlled/uncontrolled open, anchor, offset, arrow, focus tracking, outside policy | re-export; separate pure placement solver for custom floating content |
| hover_card.rs | open/close delays, anchored content | re-export |
| menu/ | ContextMenuExt, PopupMenu, DropdownMenu, keyboard navigation | re-export |
| notification.rs | six anchors, typed IDs, replace-by-ID, toast surface, dismiss events, animation | deterministic timers, hover pause, progress, visible limit and FIFO queue |
| tooltip.rs | managed tooltips, custom content, keyboard hints | re-export |
| root.rs | window-owned plugin, dialog/sheet/notification layers, focus restore | window-owned overlay plugin, no application-global window map |

`guiraffe-windows::WindowManager` accepts an arbitrary view builder. Return `guiraffe_overlays::root(view, window, cx)` from that builder after registering overlays to attach independent state to each window. The root helper reuses an existing Root entity. Native Controls APIs remain available; managed modal/drawer operations should go through this crate so bookkeeping remains consistent. Controls owns modal focus traps, modal pointer/scroll occlusion, focus restoration and transitions; GUIraffe does not create a second modal renderer.

Prior art inspected: switchboard `notifications.rs` (escape desktop markup, propagate delivery errors), `gui/hud.rs` (window-owned HUD), distrarec `ui/components.rs` (soft surface, generous padding), mocket `src` (reference application). All new presentation uses GUIraffe tokens.

## GPUI Kit 0.9.1 rebase

The foreman upgraded release/0.1 to `gpui-kit =0.9.1` during implementation.
Re-inspected the matching `gpui-component-0.9.1` and `gpui-base-0.9.1` sources:
Root plugins, dialog policies, sheets and six notification anchors retain their APIs.
Dialog additionally exposes entrance selection; GPUI renamed `on_scroll_wheel` to
`on_scroll`. Notification's dismissal-request event remains private: notifications
must be presented by its native NotificationList, which owns close/transition
completion. Managed toasts therefore use the native list, with custom content for
hover/progress and a pure scheduler for admission and lifetime. Progress ticks
notify individual notification entities, without notifying the overlay Root plugin.

## Review 1 boundary corrections

Controls WindowExt has only top-pop dialog operations and private per-dialog state.
Managed modals therefore use independent Base Dialog scopes with Controls surface
parts, preserving native dialogs and allowing removal by owned ID. Existing inferred
builder closures remain compatible; the explicit builder type is now `Modal`.
Shared motion Presence/Transition/framed owns managed enter/exit retention.

Native notifications expose no public list mutation/update API. The adapter forces
InApp admission, preserves native entities on content updates, reconciles capacity
with unmanaged notes, and queues until shared capacity is available. Native close
and notification-stack transitions remain upstream. `FloatingTooltip` measures
its actual child layout before calling the pure cross-axis placement solver.
