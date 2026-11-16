# Context-driven styling: layouts decide, widgets inherit

**Principle.** An application has a fixed suite of layouts and theme-based styles, and widgets adopt
them automatically, giving a consistent look and feel by default. Per-instance props are escape
hatches, not the normal way to style things.

## Rules
1. **Widgets have no outer spacing.** No component sets its own margin or external padding.
   Spacing *between* things belongs to the layout that contains them (gap, padding, insets).
   Widgets keep only *intrinsic* padding (a button's label inset), taken from the theme.
2. **Containers provide context; widgets read it.** A `StyleContext` flows down the tree.
   Its fields are written by layouts and containers and read by every widget:

   | Field | Example writers | Read by |
   |---|---|---|
   | `density` (compact / comfortable / spacious) | app root, Page, Table | every spacing decision |
   | `control_size` (Size) | Toolbar = Sm, Form = Md, Hero = Lg | buttons, inputs, icons, segmented |
   | `surface` (body / raised / sunken / accent) | Card, Sidebar, Header | text colour, border, hover, and contrast-safe variants |
   | `emphasis` (default / quiet) | Toolbar, table rows | default `Variant` (quiet means subtle/ghost) |
   | `align` / `wrap` hints | Group, Toolbar | how widgets handle overflow |

3. **The theme defines recipes.** For example `theme.layouts.form` is `{ gap: Md, label: above, control_size: Md }`,
   and `theme.layouts.toolbar` is `{ gap: Xs, control_size: Sm, emphasis: quiet }`.
   Apps restyle globally by editing recipes, not call sites.
4. **Escape hatches stay.** `Styled` overrides, explicit `.size()`/`.variant()` and `Space`/`Box`
   positioning always win. Lint-like debug warnings flag explicit spacing on a widget
   inside a recipe layout.

## Mechanism (GPUI)
- Stateless components (`RenderOnce`) and views render *during their parent's layout request*. So a
  `Provide<StyleContext>` element can push a frame on a per-window context stack in `request_layout`, lay out its
  children (who read `StyleContext::current(window)`), and pop. Cost: one small `Copy` struct per container,
  nothing per frame when cached.
- Cached and retained subtrees: a context change (density or scheme switch) refreshes the window,
  which is rare and acceptable. Views take the context into their cache key.
- ✅ **Verified** (`crates/core/src/gpui_tests.rs::style_context`, on both backends). Context reaches stateless
  components, nested providers and child views, and the stack is restored after each provider and after the frame.
- Providers carry a `Copy` **`StylePatch`** (optional fields) that is applied *at layout time* over the enclosing
  context. The spike showed that resolving eagerly at build time breaks nesting, because a nested provider is built
  while its parent is still rendering, before the outer provider is on the stack.

## Consequences for existing crates
- **layout:** Stack, Group, Grid, AppShell and Card become context providers with recipe defaults.
- **components:** defaults switch from hard-coded `Size::Md` to `StyleContext::current()`. `Paper` padding
  becomes the container's job.
- **conventions §2/§5** are updated: props are for overrides; the context is the default source of truth.
