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
5. **Charts**: `chart_1..5` take the primary and then Teal, Orange, Grape and Yellow (shade 6 in
   light, 5 in dark).

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
