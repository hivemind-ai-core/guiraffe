# 18 — GPUI Controls (gpui-kit 0.9.1) integration

GUIraffe sits on `gpui-kit =0.9.1`. With its default `component` feature, Kit re-exports
**GPUI Controls** (`gpui-component 0.9.1`) at `gpui_kit::component`, and charts live at
`gpui_kit::component::chart` (built on `component::plot`). GUIraffe does not re-implement what
Controls already does well. It themes those controls and puts them in the showcase, and keeps its
own widgets only where they add something (the Mantine API, StyleContext awareness, perf work).

Status legend:
- **as-is**: use the Kit control directly. GUIraffe's theme reaches it through the bridge (below).
- **wrapped**: a GUIraffe crate wraps or hosts it (crate named).
- **own**: GUIraffe ships its own equivalent (named), and the Kit one is still available.
- **gap**: nothing in GUIraffe and no Kit control, or a Kit control with a known limitation.

## Inventory

| Kit module / control | Status | Notes |
|---|---|---|
| `button::{Button, ButtonGroup, DropdownButton, Toggle, ToggleGroup}` | as-is (themed) | Variants: primary, secondary, danger, warning, success, info, ghost, link, text, outline. Also sizes, icon, loading, disabled, selected, compact. GUIraffe has `ActionIcon` (icon-only) but no text Button. |
| `input::{Input, InputState}` (text, password `masked`, multiline/textarea, code editor), `NumberInput`, `OtpInput`, `InputGroup` | as-is / wrapped | `guiraffe-ui` controls and `guiraffe-overlays` Spotlight host `Input`. GUIraffe adds `TagsInput`, `PasswordStrength` (inputs). |
| `select::{Select, SelectState}`, `combobox::Combobox` | as-is / wrapped | `guiraffe-ui` binds `Select`. |
| `checkbox::Checkbox`, `radio::{Radio, RadioGroup}`, `switch::Switch` | as-is / wrapped | `guiraffe-ui` binds Checkbox and Switch. GUIraffe `ChipGroup` is a chip-style alternative. |
| `slider::Slider` | as-is (themed) | GUIraffe has its own `RangeSlider` (inputs). |
| `rating::Rating`, `color_picker::ColorPicker` | as-is | |
| `time::{calendar::Calendar, date_picker::DatePicker, time_field::TimeField}` | as-is | |
| `tab::{Tab, TabBar}` | as-is | GUIraffe `SegmentedControl` covers segmented choice. |
| `accordion::Accordion`, `collapsible::Collapsible` | as-is | |
| `menu::{PopupMenu, ContextMenu, DropdownMenu, AppMenuBar}` | wrapped | overlays uses `ContextMenu` / `PopupMenu`. |
| `popover`, `hover_card`, `tooltip`, `dialog`, `sheet`, `notification` | wrapped | `guiraffe-overlays` puts its OverlayHost policy on top (modal, drawer, toast, tooltip). |
| `table::{DataTable, Table…}`, `list::List`, `tree::Tree`, `searchable_list` | as-is | Virtualized. Demoed with simple data. |
| `badge::Badge`, `tag::Tag` | as-is | GUIraffe `Indicator` and `Pill` overlap. |
| `progress::{Progress, ProgressCircle}`, `spinner::Spinner`, `skeleton::Skeleton`, `shimmer` | as-is (themed) | GUIraffe `RingProgress` is multi-section. Overlays `LoadingOverlay` wraps Spinner. |
| `avatar::{Avatar, AvatarGroup}`, `kbd::Kbd`, `alert::Alert`, `breadcrumb::Breadcrumb`, `pagination::Pagination`, `stepper::Stepper`, `label`, `link`, `separator`, `empty`, `description_list`, `group_box`, `toolbar`, `status_bar`, `sidebar`, `title_bar` | as-is | |
| `form::{Form, Field}`, `setting::Settings`, `questionnaire` | as-is | Kit forms. GUIraffe has no form recipe yet (gap vs 14-style-context `layouts.form`). |
| `text::TextView` (markdown/html), `highlighter`, `diff` | wrapped | `guiraffe-markdown` builds on TextView. |
| `message`, `bubble`, `message_scroller`, `attachment`, `speech` | own | `guiraffe-chat` has its own Chat / MessageView / Composer. |
| `dock`, `resizable` | wrapped | layout `Splitter` uses `ResizableState`. GUIraffe `AppShell` is its own. |
| `carousel`, `marker`, `clipboard`, `inspector` | as-is | |
| `chart::{LineChart, BarChart, AreaChart, PieChart, CandlestickChart, RadarChart, SankeyChart}` | as-is (themed) | Colors come from `chart_1..5`, which the bridge maps to the GUIraffe palette. No legend/axis-label API: label your charts in the layout around them. |
| `plot` (scales, shapes, axis, grid, tooltip) | as-is | For custom charts. |

## How GuiraffeTheme and StyleContext reach Kit controls

Kit controls read `cx.theme()` (`component::Theme`, which derefs to `ThemeColor`) plus a
`ThemeTokens` mirror. `guiraffe_core::apply()` is the single bridge:

1. **Scheme** goes to `Theme::change(ThemeMode)`, which reloads Kit's light/dark config.
2. **Primary color** goes to `primary*`, `ring`, and (since this spec) every derived field that
   Kit copies from `primary` *only when a config loads*: `button_primary*`, `progress_bar`,
   `slider_bar`, `sidebar_primary*`, `link*`, `caret`, `selection` (primary at low alpha) and
   `list_active*`. Before this fix, a Kit primary `Button`, `Slider` or `Progress` kept Kit's stock
   primary (near-black in light) while GUIraffe widgets used Blue. This was the key gap.
3. **Surfaces**: `background`, `foreground`, `border`, `input`, `muted`, `muted_foreground`,
   `popover`, `popover_foreground` and `accent` come from `GuiraffeTheme::surfaces()`, so Kit and
   GUIraffe widgets sit on the same body color in both schemes.
4. **Radius** goes to `Theme::radius` (`default_radius`).
5. **Charts**: Kit draws single-series charts (line, bar, pie) in `chart_2`, so `chart_2` is the
   primary. `chart_1,3,4,5` are Teal, Orange, Grape and Yellow (Pink stands in for whichever one is
   the primary). They use shade 6 in light and 5 in dark.

Use `Theme::update` (not `global_mut`) so Kit reconciles `ThemeTokens` and re-syncs the base layer.

**StyleContext.** Kit controls do not read `StyleContext`. Their size comes from `Sizable::with_size`.
`guiraffe_core::kit_size(Size) -> component::Size` maps GUIraffe sizes (Xs→XSmall, Sm→Small,
Md→Medium, Lg/Xl→Large). Wrappers such as `guiraffe-ui` controls call
`.with_size(kit_size(StyleContext::current().control_size))`. A bare Kit control inside a
`provide(...)` toolbar does **not** shrink automatically. That remaining gap needs either thin GUIraffe
wrappers or upstream support.

## Known Kit limitations (0.9.1)
- `ButtonVariant` has no "light" (tinted) variant like Mantine's. Use GUIraffe `ActionIcon` for that.
- Charts have no built-in legend or axis titles, and colors are global (`chart_1..5`) unless you set
  them per series with `.fill`/`.stroke`.
- Kit's `Size` has only four steps, so GUIraffe `Xl` and `Lg` look the same.

## §12 audit: interaction, focus and accessibility (spec/00 §12)

### What Kit and GPUI provide (gpui-kit 0.9.1)
- **Focus on click.** Kit `Button` calls `window.prevent_default()` on mouse down, so clicking it
  never takes focus. It does default to `tab_stop(true)`, so it is always in Tab order. That is
  opt-out, and GUIraffe's policy is opt-in. `Checkbox`, `Radio` and `Switch` have `tab_stop`/`tab_index`
  too. Text inputs take focus on click, which is correct.
- **Focus-visible.** GPUI tracks input modality (`Window::last_input_was_keyboard`). Kit draws its
  focus ring via `focus_style` whenever the element is focused, and focus is only reachable by
  keyboard when click focus is prevented. So in practice the ring shows only after a keyboard move.
- **Accessibility.** GPUI 0.3.12 embeds **AccessKit** (`gpui::Role`, `Toggled`, `AccessibleAction`).
  Divs expose `.role()`, `.aria_label()`, `aria_selected/expanded/toggled`, `aria_numeric_value*`
  and `aria_value`. Kit controls set role, label and state: Button→`Role::Button`/`Link` with
  `accessibility_label`, Checkbox/Switch toggled state, Slider numeric value, Input value/placeholder.
  **No GUIraffe widget sets a role or aria state yet.**
- **OS preferences.** `App::reduce_motion()` exists (Kit spinner/plot motion honors it). There is
  **no high-contrast or forced-colors signal** in GPUI or Kit 0.9.1. GUIraffe's `Contrast::High`
  is the theme-level substitute until the platform exposes one (tracked in KNOWN_ISSUES).

### Opt-in focus mechanism (prototype)
`StyleContext.keyboard_nav` (default false) is set by `guiraffe_core::focus_scope(child)`.
`guiraffe_core::tab_stop(explicit)` resolves per control at render time.
`guiraffe_components::ScopedButton` wraps a Kit Button with that tab stop.
Tests (`crates/components/tests/focus_scope.rs`): (1) clicking a button inside or outside a
scope keeps focus in a focused text input; (2) Tab from the input reaches only the scoped button.
Next step: every GUIraffe control reads `tab_stop(None)` and drops `window.focus` on click.

### Widget gaps
| Widget | Built on | Gaps vs §12 |
|---|---|---|
| SegmentedControl | div + `on_click` | **Click calls `window.focus`**, so it steals focus. It always tracks focus (no opt-in). No pressed or disabled state. No role (`RadioGroup`/`Tab`) or `aria_selected`. It has arrow keys. |
| ActionIcon | div + `on_click` | Not a Kit Button. It has hover only. Pressed is missing, and there is no focus or keyboard activation. Disabled is raw `opacity(0.5)`, and nothing sets a `not-allowed` cursor. No role or label (icon-only, so it needs `aria_label`). |
| Pill remove (`close_button`) | ActionIcon | Same gaps as ActionIcon, and there is no accessible "Remove X" name. |
| ChipGroup | div + `on_click` | No pressed, focus, keyboard or disabled state. No role (`CheckBox`/toggled) or selected state. |
| TagsInput | Kit Input + divs | The input is fine. Tag remove buttons have the same gaps as Pill. |
| RangeSlider | div + `on_mouse_down` | **Mouse down calls `window.focus`**. Focus is not opt-in. There is no visible focus ring, no `aria_numeric_value` and no Slider role. Disabled is not supported. It has arrow keys. |
| Dropzone | div, hover styling | Not keyboard-operable (needs Enter/Space to open a picker), no role or label, and no disabled state. |
| Showcase sidebar items | div + `on_click` | Hover only. No pressed or focus state, no arrow-key navigation, and no `ListItem`/selected role. |
| Flow toolbar | Kit `Button` | Good. Buttons should still get `tab_stop(tab_stop(None))` and the canvas needs a role. |
| Overlays (modal, drawer, spotlight, toasts) | Kit dialog/popover/Button | Mostly Kit and already fine. GUIraffe's own close and list rows in Spotlight are divs: no role or `aria_selected` on the active row. Escape works. |
| Timeline (Gantt, tracks, dope sheet, curve) | canvas + `on_mouse_down` | Pointer-only. No keyboard move/trim, no focus, no roles. Locked items use raw `opacity(0.55)`. The edit-option controls are SegmentedControls (gaps above). |

### Raw opacity / fixed shades for states (to replace with `theme.state(...)`)
- `components/src/icons.rs:146` ActionIcon disabled `opacity(0.5)`.
- `timeline/src/gantt_view.rs:528`, `tracks_view.rs:390` locked items `opacity(0.55)`.
- `overlays/src/widgets.rs:60` LoadingOverlay backdrop `a *= 0.55`, and `overlays/src/modal.rs:246`
  scrim `text.alpha(0.2)`. These are not scheme-specific; use `theme.scrim()`.
- `Text.dimmed` → `Surfaces.dimmed` (gray.6 / dark.2). Fixed shades below 4.5:1 (see report).
- Fixed error reds: `flow/src/lib.rs:832`, `gantt_view.rs:439`, `ui/src/widget.rs:248`. Fixed series
  colors: `curve_view.rs:149` (Blue.6), `dope_sheet.rs:166` (Yellow.6). These ignore scheme and primary.
- `chat/src/message.rs:209` streaming `opacity(alpha)` (animation, acceptable if reduce-motion is honored).
- Kit itself: ghost/tinted variants use `opacity(0.1/0.2/0.4)` over semantic colors, and loading uses
  `opacity(0.8)`. These are internal to Kit.

### Token API (implemented: `crates/core/src/state.rs`)
`theme.state(StateKind, Variant, Option<ColorName>) -> StateColors { foreground, background, border }`
for `Rest, Hover, Pressed, Selected, Disabled, Dimmed, FocusRing`. Backgrounds are composited over
the body, so ratios can be measured. `GuiraffeTheme.contrast: Contrast::{Standard, High}`.
`theme.scrim()` is scheme-aware. `contrast_checks(&theme)` lists every §12.1 floor.

### Contrast report (test `state::tests::contrast_report_every_state_scheme_and_primary`)
The test covers 12 primaries × 2 schemes × 2 contrast settings. It fails today: **302 (standard) and 188
(high)**. The counts are pinned in the test, so a fix or a regression both have to be acknowledged.
I did not tune any colors. Lowest ratio per group:
- **Dimmed text**: light 3.32 (gray.6 on white) and dark 4.04, against a floor of 4.5. High contrast passes.
- **Disabled text** (light): 2.99 against a floor of 3.0, so just under.
- **Control boundaries** (Default/Outline border vs body): light 1.49–1.86, dark 1.54, against a floor of 3.0. Mantine's gray.3/4 borders are decorative-strength.
- **Filled text** (white on shade 6): as low as 3.28 in light and 3.45 in dark for light hues (yellow, lime, cyan, orange…). `readable_foreground` only switches to black for some of them.
- **Light/Outline/Subtle text** (shade-6 text on a tint) in light: as low as 1.6–1.9 for yellow, lime and cyan primaries. Blue and violet pass.
- Pressed and hover drop up to 0.5 below rest.
- The focus ring passes everywhere (it falls back to text color when the primary is too light).

## Contrast report — resolved (2026-11-17)
All §12.1 floors now pass for both schemes, all 12 primaries, Standard and High contrast (was 302/188 failures), enforced by `state::tests::contrast_report_every_state_scheme_and_primary`. Changes, following Refactoring UI ("keep the hue, move along its scale"; "flip the contrast"):
- Tinted text (Light/Outline/Subtle) walks the hue's shade scale (darker in light mode, lighter in dark) until ≥5:1 on the strongest tint it sits on (`variant::readable_shade`).
- Filled: prefer white text, deepening the fill up to two shades until white reaches 4.5:1 (default blue 6 → 8); hues too bright even then (yellow, lime) flip to dark text. Hover moves away from the text color.
- Pressed continues the rest→hover direction instead of blending toward the text.
- Dimmed text: light = halfway gray 6→7 (≥4.5:1), dark = dark 1.
- **Exemption (decision for the user):** Standard-contrast Default control borders stay subtle (≈1.5:1, Mantine/Refactoring UI style); High contrast meets 3:1. Making Standard compliant means a gray-6-ish border on every default button/input.
