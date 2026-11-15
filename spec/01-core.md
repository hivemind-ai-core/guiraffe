# guiraffe-core
- `ColorName` (dark, gray, red, pink, grape, violet, indigo, blue, cyan, teal, green, lime, yellow, orange) each with 10 shades (Mantine defaults), `primary_color`, `primary_shade` per scheme.
- `Size` {Xs,Sm,Md,Lg,Xl} → spacing, radius, font size, control height tables.
- `Variant` {Filled, Light, Outline, Subtle, Transparent, White, Default, Gradient}; `variant_colors(theme, color, variant) -> {bg, hover, fg, border}` pure + tested.
- `GuiraffeTheme` global: scheme (light/dark), font families, shadows (xs..xl), radius default, spacing; bridge to/from Controls' `Theme` so Controls widgets match.
- Style helper traits (`.p_size(Size)`, `.radius(Size)`, `.shadow_size(Size)`).
- Theme-switch with no flicker; tests for contrast (WCAG AA for filled text).
