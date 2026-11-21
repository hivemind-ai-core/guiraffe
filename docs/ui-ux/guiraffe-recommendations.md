# GUIraffe design improvements

Baseline inspected: `ac49cc5` on `release/0.1`. These proposals use existing theme and verification infrastructure. Confirm current gaps against the assigned release revision before starting an implementation ticket.

## First batch

| Priority and owner | Starting point and proposed change | Acceptance evidence |
|---|---|---|
| P0 · Core and Components | Start with `Press`, `StyleContext` and the Controls integration audit in `spec/18-controls-integration.md`. Verify which controls already consume the shared layer. Close remaining gaps in icon actions and composite selection controls. Document when toolbars preserve editor focus on pointer use and when a control must take focus. | Exercise pointer use with an active editor, Tab and Shift+Tab, Enter/Space, composite arrow keys and disabled activation. Inspect accessible name and state on actual platforms. |
| P0 · Core and Showcase | Extend the current semantic contrast checks and gallery coverage to approved surface/state pairings. Include focus on selection, neutral versus destructive emphasis, tinted backgrounds and inactive-window presentation. Keep existing Kit mapping as the integration point. | Table-driven tests use resolved foreground/background pairs. Captures show a representative settings screen and dense toolbar in light, dark and high contrast. Distinguish measured contrast from visual judgment. |
| P1 · Layout and Core | Use the existing `Recipes`, `Density` and layout context. Pilot settings, toolbar and inspector compositions before expanding public API. Define what each container owns, how controls inherit size, and which action moves to overflow at narrow widths. Evaluate role-based text defaults against current theme helpers. | A fixture changes density and theme at the root. Long labels, narrow widths and large text retain usable controls. Explicit caller overrides win. Nested providers and sibling restoration pass on both backends. |
| P1 · Overlays and Showcase | Define a dismissal and focus-restoration contract for modal, popover and spotlight. Demonstrate recovery inside a realistic export or settings workflow. Reuse Kit overlays and verify GUIraffe-owned trigger, action and close elements. | Escape, outside click where allowed, commit, cancel, nested popovers, validation failure and deletion of the opener each have an interaction check. |
| P1 · Showcase and Test | Add workflow stories to existing widget coverage. Start with a searchable list, edit form and command or overlay flow. Use deterministic data and pair visual captures with event assertions. | Store revision, backend, scale, theme, viewport and state with review evidence. Exercise first use, long names, empty search, async failure and recovery. Keep a manual platform accessibility record alongside automated results. |

## Follow-up batch

- **DataGrid, Flow and Timeline.** Design keyboard-first row and object selection with stable IDs through sort, filtering and virtualization. For editing gestures, supply equivalent commands for move, trim or connect where applicable. Start with one complete journey per crate. Verify cancellation, undo, focus and scroll continuity across refresh.
- **Motion.** Use existing transition and presence APIs for an interruptible overlay or disclosure recipe. Define allowed motion and a direct reduced-motion path. Test rapid reversal, removal and preference changes. Profile frame-time distribution during realistic input on representative hardware and record backend and scale. Use the framework's 120 fps goal as an approximately 8.33 ms display-frame budget, with separate evidence for CPU work and displayed frames.
- **App examples.** Document a healthy composition from the app shell through a dense content surface, form, and overlay. Demonstrate quiet secondaries, clear selection, one dominant action per decision, realistic error copy and reduced motion. Include an example using an unwrapped Kit control so authors can see where the theme bridge applies.
- **Theme interoperability.** Evaluate design-token exchange only when a real external producer or consumer exists. Prototype a small set of semantic aliases and validate both theme resolution and compatibility with existing Rust callers. [Design Tokens Community Group format](https://www.designtokens.org/tr/2025.10/format/).

## Pilot and rollout

1. Assign crate ownership and capture one settings workflow and one dense workspace at the release revision. Start with an audit of normal entry points so a showcase-only fix cannot masquerade as a framework default.
2. Land narrow improvements through crate worktrees. Preserve the `gpui-kit =0.9.1` workspace pin. Keep pure style and state resolution unit-testable and use `TestAppContext` for interaction. Use the crate test, Clippy and formatting gates for implementation.
3. Compare pilot captures and interactions under common light/dark conditions and selected high-contrast, DPI, reduced-motion and long-text combinations. Use existing `crates/showcase/tests` rendering and keyboard checks. Record platform verification independently of snapshot and test results.
4. Validate with users who do the workflow. For settings, check unaided completion, field-error recovery and keyboard completion. For the dense workspace, check time and errors on selection or command use. Roll shared defaults outward when pilots improve the selected outcomes and existing callers retain predictable overrides.

Open separate tickets for cross-crate work and coordinate with the foreman. The priority order favors operability and coherent defaults so application teams get improvements from ordinary component use.
