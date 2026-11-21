# guiraffe-overlays
- Overlay stack manager per window: z-ordering, focus trap & restore, Esc/outside-click policy, scroll lock, enter/exit animations.
- `modals` manager: open/confirm/context modals, stacked modals, async result (`await` confirm).
- Drawer (4 sides), Popover/HoverCard/Menu thin wrappers, ContextMenu, Tooltip floating positioning (flip/shift pure + tested).
- Notifications (toasts): positions, auto-close, progress, update by id, limit+queue.
- Spotlight (command palette) with fuzzy search.
- Desktop notifications (notify-rust on Linux, platform fallbacks), feature flag `desktop`. See switchboard.
- LoadingOverlay, Affix.

## Dismissal and focus contract

- Escape dismisses the focused popover or the top modal or Spotlight. Modal
  policies can disable Escape. A popover inside a modal consumes its own Escape,
  returning focus to the control that had focus in the modal before opening.
- Outside press dismisses a modal or popover only when its outside-dismiss policy
  allows it. Spotlight allows outside dismissal. Confirm dialogs disable it.
- An accepted modal commit closes the modal and resolves a confirmation as
  accepted. A rejected commit leaves the modal open with focus inside it.
  Cancel and Close dismiss without committing, subject to the modal's cancel
  handler. Spotlight selection closes the palette and runs the command.
  Popover actions decide whether to call Kit's dismiss operation.
- Managed overlays restore focus to the previously focused control if it still
  belongs to the exposed overlay or window. If that opener was removed, focus
  goes to the exposed overlay's focus scope or the window content scope. Kit
  popovers restore the prior focus when they still own focus at dismissal.
