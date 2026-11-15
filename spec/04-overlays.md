# guiraffe-overlays
- Overlay stack manager per window: z-ordering, focus trap & restore, Esc/outside-click policy, scroll lock, enter/exit animations.
- `modals` manager: open/confirm/context modals, stacked modals, async result (`await` confirm).
- Drawer (4 sides), Popover/HoverCard/Menu thin wrappers, ContextMenu, Tooltip floating positioning (flip/shift pure + tested).
- Notifications (toasts): positions, auto-close, progress, update by id, limit+queue.
- Spotlight (command palette) with fuzzy search.
- Desktop notifications (notify-rust on Linux, platform fallbacks), feature flag `desktop`. See switchboard.
- LoadingOverlay, Affix.
