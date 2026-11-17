# Composable charts

`guiraffe-charts` composes ordered marks with independent typed sources, channel
accessors, shared positional scales and guides. It follows the mark-local data
model in [TanStack Charts](https://tanstack.com/charts/latest/docs/concepts/grammar-of-graphics).
Use Kit's chart components for standalone charts that need only their controls.

## Data and encodings

A `ChartSource` returns borrowed rows by index and stable keys. A `Layer` binds
the source to typed accessors for x, y, group, color, size, optional bounds,
OHLC, text and facet. Each layer can use a different row type. Preparation
reads borrowed rows and owns only the numeric encodings and label strings
needed for rendering. Selection reports the layer and original row identity.

`Stream` publishes immutable, shared chunks with stable row indices. Appending
moves the incoming rows into the next chunk and shares existing chunks with
readers. Prepare a replacement chart from a snapshot on a background executor
and publish only the latest completed generation to the chart entity.

## Scales and marks

Linear, logarithmic and epoch-second time scales map continuous domains.
Band and ordinal scales use zero-based indices into an ordered category list.
Band positions are centered in padded slots and ordinal positions are evenly
spaced points. Color can encode a category or a normalized continuous value.
Themes supply the categorical palette with a contrast check against the plot
surface. Lines and grouped legend labels supply visual context beyond hue.

Mark geometry includes line, area, bar, point, bubble, rect, rule, text,
error bar and candlestick. Area and bar accept overlap, stack or normalize.
Grouping splits connected marks and provides slots for grouped bars. Stacks
are local to one layer and facet, aligned by exact category/x key, with
independent positive and negative totals. Duplicate x values in one stack
group contribute in source order. Facets share the x domain and primary and
secondary y domains. Secondary layers opt into the right axis.

Axis titles, ticks and grids explain the shared domains. Continuous time uses
the timeline ruler's tick ladder and date formatter. Text and rule layers
place annotations in data coordinates. Legend buttons toggle series and
support keyboard operation.

## Rendering, interaction and accessibility

The chart compiles the grammar to a viewport scene. Sorted x indices permit
binary-search culling. Large line runs use Largest Triangle Three Buckets
(LTTB), preserving endpoints and source keys. Canvas painting uses paths and
quads. The plot caches geometry by chart generation, viewport, visibility and
dimensions. Hover reads the cached scene and uses a chart-local overlay;
transient interaction leaves the prepared data intact.

Crosshair and nearest-point tooltip operate on enabled marks. Brush changes
the x viewport and emits the selected domain. Wheel zoom is anchored to the
pointer, and horizontal scrolling pans. Selection emits stable row identity.
The chart exposes a named AccessKit canvas and an alternate paged data table
with keyboard-operable controls. Table content is limited to one page of
rows, independent of source size.

The benchmark suite measures prepared model culling and scene generation.
Headless interaction tests exercise both GPUI backends. Gallery snapshots use
separate light and dark fixtures for each backend.
