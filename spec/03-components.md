# guiraffe-components (gaps vs Controls; check Controls first, wrap if it exists)
Inputs: SegmentedControl, Chip/ChipGroup, PinInput, NumberInput w/ steppers, TagsInput, MultiSelect pills, Autocomplete, FileInput/Dropzone, ColorInput+swatches, Rating(if missing), RangeSlider, JsonInput, PasswordInput strength meter.
Display: Card (sections), Paper, ThemeIcon, Indicator, RingProgress, SemiCircleProgress, Timeline, Stepper polish, Spoiler, Highlight, Mark, Blockquote, Code/CodeBlock (copy), Image w/ fallback, BackgroundImage, Table helpers, Title/Text typography scale, Anchor, NavLink, Pill, Loader variants (oval, bars, dots), Burger, CloseButton, ActionIcon, CopyButton, UnstyledButton.
Agent chat (inspired by mocket): message list (virtualized, streaming markdown), tool-call cards (collapsible), composer with attachments/mentions/slash-commands, thinking blocks.

## Required: syntax-highlighting code viewer

- A read-only `CodeBlock` / code viewer component with language-aware syntax highlighting. Editing is deferred.
- Reuse or wrap Controls support first; keep the viewer usable as a standalone component and embedded in Markdown fenced code blocks, including agent chat. Markdown must reuse this viewer rather than duplicate highlighting/rendering logic.
- Selectable, copyable source text; optional language label and line numbers; horizontal scrolling for long lines.
- Theme-aware highlighting for light and dark schemes, with a plain-text fallback for unknown languages.
- Cache highlighting outside the render path and keep large source files responsive.
- Acceptance: tokenization/fallback tests, GPUI selection/copy interaction tests, and light/dark gallery examples.

## Advanced (spec/16)
- Code viewer with annotations, trace lines, bubbles, symbols, lens and minimap
- Code canvas / code bubbles (CodeCanvas-style, built on flow)
- Git history graph (GitKraken-style)
- Hex view and assembler view
- Timeline family: Gantt, DAW/animation timeline, dope sheet, curve editor
