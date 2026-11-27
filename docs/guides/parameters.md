# Parameters: knobs, number fields and value mapping

> **Provisional (0.3.1).** Built for Passage's device cards. The names can still change
> before 0.4.

`KnobControl`, `NumberFieldControl` and `SliderControl` share one value mapping, so a
cutoff reads, drags, steps and parses the same way in all three.

## Value mapping

```rust
use guiraffe_components::mapping::{Curve, Unit, ValueMap};

let cutoff = ValueMap::new(20.0, 20_000.0)
    .curve(Curve::Log)        // equal travel, equal ratio
    .unit(Unit::Hertz)        // 440 Hz, 1.25 kHz
    .default_to(1_000.0);     // double-click and reset return here
let pan = ValueMap::new(-1.0, 1.0).bipolar(true).decimals(2);
let semitones = ValueMap::new(-24.0, 24.0).step(1.0).unit(Unit::Semitones);
```

| Part | What it does |
|---|---|
| `Curve::Linear`, `Curve::Log`, `Curve::Exp(k)` | How travel maps to the value. `Log` needs a positive range; one that touches zero falls back to linear. |
| `.step(s)` | Values snap to multiples of `s` from `min`. It is also the keyboard and `increment` step. Without it, a step is a hundredth of the travel (a thousandth when fine). |
| `.default_to(v)` | The reset value. |
| `.unit(u)` | Formatting and the suffixes typed entry accepts. |
| `.decimals(n)` | Fixed precision instead of the unit's adaptive one. |
| `.bipolar(true)` | Centred: the value arc grows from the centre (0, or mid-travel) and drags detent there. |

Units:

| `Unit` | Shows | Typed entry |
|---|---|---|
| `Hertz` | `440 Hz`, `1.25 kHz` | `440`, `880 Hz`, `2.5k`, `2.5 kHz` |
| `Seconds` | `250 ms`, `1.5 s` | `0.25`, `250 ms`, `1.5 s` |
| `Decibels` | `-6.0 dB`, `+3.0 dB`, `-inf dB` (at `min <= -96`) | `-6`, `-6 dB`, `-inf` |
| `Percent` (value `0..=1`) | `50%` | `50%`, `50`, `0.5` |
| `Semitones` | `+7 st` | `7`, `+7 st` |
| `Note` (MIDI number) | `C4`, `F#3` | `C4`, `F#3`, `Db5`, `60` |
| `None` | `0.75` | a number |

Typed entry is clamped and snapped like any write. Text that is not a number, or a suffix
the unit doesn't know, is a `ParseError`.

`ValueMapping` is the trait behind `ValueMap`. Implement it for a custom scale (a table of
detents, a tempo-synced rate) and pass it with `.mapping(…)`.

## KnobControl

```rust
let knob = KnobControl::new()
    .mapping(cutoff)
    .label("Cutoff")            // read as "Cutoff: 1.25 kHz"
    .modulation(Some((400.0, 4_000.0)))
    .diameter(48.)
    .path("synth/filter/cutoff")
    .build(window, cx);
```

- Drag vertically: 200 px covers the range. Shift drags at a tenth of that speed. A drag
  previews locally and commits one `set_value` in a `dispatch_batch` on release, so it is
  one undo step.
- Scroll steps up and down. The arrow keys step (Shift for fine), and Home and End go to the
  ends. A double-click resets to the default.
- Bipolar mappings draw from the centre and detent there (a fine drag skips the detent).
- `modulation` draws an outer arc over a range in value units. `set_modulation` updates it.
- The element is a slider with a numeric value, min, max and the formatted text. Its bus
  snapshot's label is `Cutoff: 1.25 kHz`.
- `.compact(true)` shows the dial only, for dense rows such as per-track levels. The value
  appears in the tooltip, and above the dial while dragging. The accessible value is unchanged.

## NumberFieldControl

```rust
let field = NumberFieldControl::new().mapping(cutoff).label("Cutoff").build(window, cx);
```

It shows the formatted value. Drag up or right to scrub (Shift for fine). The arrow keys
step. Double-click or Enter opens typed entry, which parses through the mapping. Enter
commits. Text that doesn't parse keeps entry open, with `error()` saying why.

- `.default_value(v)` gives the field a reset: Alt- or Ctrl-click sets it back to `v`.
  (Double-click is typed entry here, so a reset takes a modifier click.) The reset is a
  `set_value` on the bus, so it is one undo step and interceptors see it like any other edit.
- `.size(Size::Xs | Sm | Md)` follows the shared control scale: height, padding and text size.
  `Sm` is the default, and `Xs` fits compact toolbars.

## On the bus and in `ui!`

Both controls take `set_value` (num), `increment` and `decrement`. Every parameter can be
bound to shortcuts, Spotlight and the ribbon without a custom action. Events are
`ParamEvent::Changed(value, Source)`. Host setters (`set_value`, `set_modulation`,
`set_disabled`, `set_label`) need no window and never emit, so a host write never echoes.

```rust
ui! {
    store(synth) {
        KnobControl { value <=> cutoff  .mapping(cutoff_map)  .label("Cutoff")  .path("synth/cutoff") }
        NumberFieldControl { value <=> attack  .mapping(attack_map) }
    }
}
```

Bound values are `f64`. A binding update allocates nothing (gated in `guiraffe-bench`,
`tests/knob.rs`), and an unbound knob re-renders for what a plain element of the same
shape costs.

## Sliders

`SliderControl` takes the same mapping with `.mapping(…)`, and its kit slider then runs in
normalised travel, so a log Hz slider drags and steps like a knob. `.range(min, max)` and
`.step(s)` keep 0.2 behaviour: the kit state in value units, host writes clamped but not
snapped.
