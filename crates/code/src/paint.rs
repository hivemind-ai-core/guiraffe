use crate::Position;
use crate::{surface::Shared, *};
use gpui_kit::*;
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, StyleContext};
use std::{
    collections::{HashMap, VecDeque},
    fmt::Write,
    rc::Rc,
};

/// Maximum UTF-8 bytes shaped for one logical line. Full source remains copyable.
pub const MAX_SHAPED_LINE_BYTES: usize = 16_384;
fn prefix(text: &str) -> &str {
    let mut end = text.len().min(MAX_SHAPED_LINE_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
fn bounded_runs(runs: Vec<TextRun>, len: usize) -> Vec<TextRun> {
    let mut remaining = len;
    runs.into_iter()
        .filter_map(|mut run| {
            run.len = run.len.min(remaining);
            remaining -= run.len;
            (run.len > 0).then_some(run)
        })
        .collect()
}

const STYLE_OVERSCAN_ROWS: usize = 8;
const MAX_SHAPE_CACHE_ENTRIES: usize = 2_048;
const MAX_SHAPE_CACHE_TEXT_BYTES: usize = 256 * 1024;

type ShapeKey = (u64, u64, usize, u32, u32);

#[derive(Default)]
pub(crate) struct ShapeCache {
    items: HashMap<ShapeKey, Rc<ShapedLine>>,
    order: VecDeque<ShapeKey>,
    text_bytes: usize,
    frame: Rc<std::cell::RefCell<Vec<PaintedRow>>>,
    gutter_label: String,
    styled_viewport: Option<(std::ops::Range<usize>, u64)>,
}
impl ShapeCache {
    fn retain_viewport_styles(
        &mut self,
        map: &RowMap,
        visible: &std::ops::Range<usize>,
        epoch: u64,
        styled: bool,
    ) {
        let next = styled.then(|| {
            (
                visible.start.saturating_sub(STYLE_OVERSCAN_ROWS)
                    ..visible
                        .end
                        .saturating_add(STYLE_OVERSCAN_ROWS)
                        .min(map.len()),
                epoch,
            )
        });
        if self.styled_viewport == next {
            return;
        }
        let keep = |key: &ShapeKey| {
            let (hash, shape_epoch, line, _, _) = *key;
            // Gutter labels and shared plain text do not carry syntax styles.
            hash != 0
                || line == usize::MAX
                || next.as_ref().is_some_and(|(rows, epoch)| {
                    shape_epoch == *epoch
                        && map.source_row(line).is_some_and(|row| rows.contains(&row))
                })
        };
        self.items.retain(|key, shape| {
            let retain = keep(key);
            if !retain {
                self.text_bytes -= shape.text.len();
            }
            retain
        });
        self.order.retain(keep);
        self.styled_viewport = next;
    }

    fn line(
        &mut self,
        text: &str,
        key: ShapeKey,
        runs: impl FnOnce() -> Vec<TextRun>,
        font_size: f32,
        window: &mut Window,
        shared: &Shared,
    ) -> Rc<ShapedLine> {
        let text = prefix(text);
        let mut stats = shared.stats.get();
        stats.max_shaped_bytes = stats.max_shaped_bytes.max(text.len());
        if let Some(shape) = self.items.get(&key)
            && shape.text.as_ref() == text
        {
            stats.cache_hits += 1;
            shared.stats.set(stats);
            return shape.clone();
        }
        let shape = Rc::new(window.text_system().shape_line(
            text.to_owned().into(),
            px(font_size),
            &bounded_runs(runs(), text.len()),
            None,
        ));
        stats.shaped_lines += 1;
        shared.stats.set(stats);
        self.insert(key, shape.clone());
        shape
    }

    fn insert(&mut self, key: ShapeKey, shape: Rc<ShapedLine>) {
        if let Some(old) = self.items.remove(&key) {
            self.text_bytes -= old.text.len();
            self.order.retain(|candidate| *candidate != key);
        }
        let bytes = shape.text.len();
        while self.items.len() >= MAX_SHAPE_CACHE_ENTRIES
            || self.text_bytes + bytes > MAX_SHAPE_CACHE_TEXT_BYTES
        {
            let Some(old_key) = self.order.pop_front() else {
                return;
            };
            if let Some(old) = self.items.remove(&old_key) {
                self.text_bytes -= old.text.len();
            }
        }
        self.text_bytes += bytes;
        self.order.push_back(key);
        self.items.insert(key, shape);
    }
}
pub(crate) struct PaintedRow {
    shape: Rc<ShapedLine>,
    origin: Point<Pixels>,
    gutter: bool,
}
pub(crate) struct TextFrame {
    rows: Rc<std::cell::RefCell<Vec<PaintedRow>>>,
    line_height: Pixels,
    code_bounds: Bounds<Pixels>,
    bounds: Bounds<Pixels>,
    background: Hsla,
}
fn color(name: ColorName, theme: &GuiraffeTheme) -> Hsla {
    name.shade(if theme.scheme == Scheme::Dark { 4 } else { 7 })
        .to_hsla()
}
fn base_run(text: &str, color: Hsla, font: &Font) -> TextRun {
    TextRun {
        len: text.len(),
        font: font.clone(),
        color,
        ..Default::default()
    }
}
fn runs(
    shared: &Shared,
    line: usize,
    text: &str,
    theme: &GuiraffeTheme,
    font: &Font,
) -> Vec<TextRun> {
    let highlighted = shared.highlighted.borrow();
    let Some(mut range) = highlighted.document.line_range(line) else {
        return vec![];
    };
    let source = prefix(highlighted.document.line(line).unwrap_or_default());
    range.end = range.start + source.len();
    let hints = shared.hints.borrow();
    let display = hints.get(&line);
    let mut points = std::collections::BTreeSet::from([0, source.len()]);
    if let Some(display) = display {
        for (column, _) in display.insertions() {
            points.insert(*column);
        }
    }
    let styles: Vec<_> = highlighted
        .styles_for_scheme(range.clone(), theme.scheme == Scheme::Dark)
        .collect();
    for (r, _) in &styles {
        points.insert(r.start.max(range.start) - range.start);
        points.insert(r.end.min(range.end) - range.start);
    }
    let mut ids = Vec::new();
    shared
        .index
        .borrow()
        .query(range.start..range.end.max(range.start + 1), &mut ids);
    let annotations = shared.annotations.borrow();
    let decorations: Vec<_> = ids
        .iter()
        .filter_map(|i| annotations.get(*i))
        .filter(|a| {
            matches!(
                a.kind,
                AnnotationKind::Highlight | AnnotationKind::Underline
            )
        })
        .filter_map(|a| highlighted.document.range(a.range).map(|r| (a, r)))
        .collect();
    for (_, r) in &decorations {
        points.insert(r.start.max(range.start).min(range.end) - range.start);
        points.insert(r.end.min(range.end).max(range.start) - range.start);
    }
    let points: Vec<_> = points.into_iter().collect();
    let mut result = Vec::new();
    for pair in points.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            continue;
        }
        let mut run = base_run(&source[start..end], theme.surfaces().text.to_hsla(), font);
        let absolute = range.start + start;
        let style_index = styles.partition_point(|(r, _)| r.end <= absolute);
        if let Some((_, style)) = styles
            .get(style_index)
            .filter(|(r, _)| r.contains(&absolute))
        {
            if let Some(c) = style.color {
                run.color = c;
            }
            if let Some(weight) = style.font_weight {
                run.font.weight = weight;
            }
            if let Some(style) = style.font_style {
                run.font.style = style;
            }
        }
        for (a, r) in &decorations {
            if r.contains(&absolute) {
                let c = color(a.color, theme);
                match a.kind {
                    AnnotationKind::Highlight => run.background_color = Some(c.opacity(0.15)),
                    AnnotationKind::Underline => {
                        run.underline = Some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(c),
                            wavy: true,
                        })
                    }
                    _ => {}
                }
            }
        }
        if let Some(display) = display {
            for (_, range) in display.insertions().filter(|(column, _)| *column == start) {
                if let Some(hint) = text.get(range.start..range.end.min(text.len())) {
                    result.push(base_run(hint, theme.surfaces().dimmed.to_hsla(), font));
                }
            }
        }
        result.push(run);
    }
    if let Some(display) = display {
        for (_, range) in display
            .insertions()
            .filter(|(column, _)| *column == source.len())
        {
            if let Some(hint) = text.get(range.start..range.end.min(text.len())) {
                result.push(base_run(hint, theme.surfaces().dimmed.to_hsla(), font));
            }
        }
    }
    result
}
pub(crate) fn prepare(
    shared: &Shared,
    cache: &mut ShapeCache,
    bounds: Bounds<Pixels>,
    theme: &GuiraffeTheme,
    context: StyleContext,
    window: &mut Window,
    cx: &mut App,
) -> TextFrame {
    let options = shared.options.borrow();
    let token_size = options.size.unwrap_or(context.control_size);
    let scale = if theme.scale.is_finite() {
        theme.scale.clamp(0.1, 10.)
    } else {
        1.
    };
    let font_size = token_size.font_size() * scale;
    let family = cx
        .global::<gpui_kit::component::Theme>()
        .mono_font_family
        .clone();
    let font = font(family.clone());
    let font_key = content_hash(family.as_bytes());
    let lh = font_size * token_size.line_height();
    let digits = shared
        .highlighted
        .borrow()
        .document
        .line_count()
        .max(1)
        .ilog10() as usize
        + 1;
    let cell = cache
        .line(
            "M",
            (font_key, 0, 0, font_size.to_bits(), 99),
            || vec![base_run("M", theme.surfaces().text.to_hsla(), &font)],
            font_size,
            window,
            shared,
        )
        .width
        .as_f32();
    let gutter = (if options.line_numbers { digits + 3 } else { 2 }
        + shared
            .gutters
            .borrow()
            .iter()
            .map(|c| c.width_chars.min(64) + 1)
            .sum::<usize>()) as f32
        * cell;
    let mini = if options.minimap { cell * 8. } else { 0. };
    let margin =
        (options.margin_chars as f32 * cell).min((bounds.size.width.as_f32() * 0.4).max(0.));
    let mut geometry = shared.geometry.borrow_mut();
    geometry.bounds = bounds;
    geometry.layout_epoch = shared.layout_epoch.get();
    geometry.font_size = font_size;
    geometry.line_height = lh;
    geometry.gutter = gutter;
    geometry.margin = margin;
    geometry.mini = mini;
    geometry.font = font.clone();
    geometry.shaped.clear();
    let map = shared.rows.borrow();
    let mut viewport = shared
        .viewport
        .get()
        .clamped(map.len(), lh, bounds.size.height.as_f32());
    let width = (bounds.size.width.as_f32() - gutter - margin - mini).max(0.);
    viewport.x = viewport
        .x
        .min((geometry.content_width - f64::from(width)).max(0.));
    shared.viewport.set(viewport);
    let visible = viewport.visible(map.len(), lh, bounds.size.height.as_f32());
    let mut stats = shared.stats.get();
    stats.last_visible_rows = visible.len();
    stats.last_truncated_rows = 0;
    shared.stats.set(stats);
    let highlighted = shared.highlighted.borrow();
    let doc = &highlighted.document;
    let frame = cache.frame.clone();
    let mut rows = frame.borrow_mut();
    rows.clear();
    let surface_color = theme.surfaces().text;
    let epoch = shared.text_epoch.get()
        ^ font_key
        ^ u64::from(u32::from_be_bytes([
            surface_color.r,
            surface_color.g,
            surface_color.b,
            surface_color.a,
        ]));
    cache.retain_viewport_styles(&map, &visible, epoch, highlighted.has_parser());
    let appearance = if theme.scheme == Scheme::Dark { 1 } else { 0 };
    let s = theme.surfaces();
    let mut label = std::mem::take(&mut cache.gutter_label);
    for row in visible {
        let y = bounds.origin.y + px((row as f64 * f64::from(lh) - viewport.y) as f32);
        match map.row(row) {
            Some(Row::Source(line)) => {
                let hints = shared.hints.borrow();
                let text = hints
                    .get(&line)
                    .map(|d| d.text.as_ref())
                    .unwrap_or_else(|| doc.line(line).unwrap_or_default());
                if text.len() > MAX_SHAPED_LINE_BYTES {
                    let mut stats = shared.stats.get();
                    stats.last_truncated_rows += 1;
                    shared.stats.set(stats);
                }
                let text = prefix(text);
                // Short, unstyled lines can share shaping across source rows.
                let (hash, context_line) = if text.len() <= 64
                    && !highlighted.has_parser()
                    && shared.annotations.borrow().is_empty()
                {
                    (content_hash(text.as_bytes()), usize::MAX)
                } else {
                    (0, line)
                };
                let shape = cache.line(
                    text,
                    (hash, epoch, context_line, font_size.to_bits(), appearance),
                    || runs(shared, line, text, theme, &font),
                    font_size,
                    window,
                    shared,
                );
                geometry.content_width =
                    geometry.content_width.max(f64::from(shape.width.as_f32()));
                geometry.shaped.insert(line, shape.clone());
                rows.push(PaintedRow {
                    shape,
                    origin: point(bounds.origin.x + px(gutter - viewport.x as f32), y),
                    gutter: false,
                });
                let folds = shared.folds.borrow();
                let marker = folds
                    .get(folds.partition_point(|fold| fold.start < line))
                    .filter(|fold| fold.start == line)
                    .map(|fold| if fold.collapsed { "+" } else { "−" })
                    .unwrap_or(" ");
                label.clear();
                if options.line_numbers {
                    let _ = write!(label, "{:>digits$} {marker}", line + 1);
                } else {
                    label.push_str(marker);
                }
                for column in shared.gutters.borrow().iter() {
                    let value = column.cells.get(&line).map(|s| s.as_ref()).unwrap_or("");
                    let width = column.width_chars.min(64);
                    label.push(' ');
                    let mut used = 0;
                    for ch in value
                        .chars()
                        .filter(|c| *c != '\n' && *c != '\r')
                        .take(width)
                    {
                        label.push(ch);
                        used += 1;
                    }
                    label.extend(std::iter::repeat_n(' ', width - used));
                }
                let label_hash = content_hash(label.as_bytes());
                let label_shape = cache.line(
                    &label,
                    (
                        label_hash,
                        0,
                        usize::MAX,
                        font_size.to_bits(),
                        appearance + 2,
                    ),
                    || vec![base_run(&label, s.dimmed.to_hsla(), &font)],
                    font_size,
                    window,
                    shared,
                );
                rows.push(PaintedRow {
                    shape: label_shape,
                    origin: point(bounds.origin.x, y),
                    gutter: true,
                });
            }
            Some(Row::Lens(id)) => {
                if let Some(a) = shared.annotations.borrow().get(id) {
                    let text = prefix(&a.payload).replace(['\n', '\r'], " ");
                    let shape = cache.line(
                        &text,
                        (
                            content_hash(text.as_bytes()),
                            epoch,
                            id,
                            font_size.to_bits(),
                            appearance + 4,
                        ),
                        || vec![base_run(&text, color(a.color, theme), &font)],
                        font_size,
                        window,
                        shared,
                    );
                    rows.push(PaintedRow {
                        shape,
                        origin: point(bounds.origin.x + px(gutter), y),
                        gutter: false,
                    });
                }
            }
            None => {}
        }
    }
    cache.gutter_label = label;
    drop(rows);
    TextFrame {
        rows: frame,
        line_height: px(lh),
        code_bounds: Bounds::new(
            point(bounds.origin.x + px(gutter), bounds.origin.y),
            size(
                (bounds.size.width - px(gutter + margin + mini)).max(px(0.)),
                bounds.size.height,
            ),
        ),
        bounds,
        background: s.body.to_hsla(),
    }
}
pub(crate) fn paint_text(frame: TextFrame, window: &mut Window, cx: &mut App) {
    window.paint_quad(fill(frame.bounds, frame.background));
    for row in frame.rows.borrow().iter() {
        let mask = if row.gutter {
            Bounds::new(
                frame.bounds.origin,
                size(
                    frame.code_bounds.origin.x - frame.bounds.origin.x,
                    frame.bounds.size.height,
                ),
            )
        } else {
            frame.code_bounds
        };
        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
            let _ = row.shape.paint_background(
                row.origin,
                frame.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
            let _ = row.shape.paint(
                row.origin,
                frame.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        });
    }
}
pub(crate) fn hit(shared: &Shared, point: Point<Pixels>) -> Option<Position> {
    let g = shared.geometry.borrow();
    if !g.bounds.contains(&point) || g.line_height <= 0. {
        return None;
    }
    let viewport = shared.viewport.get();
    let y = f64::from(point.y.as_f32() - g.bounds.origin.y.as_f32()) + viewport.y;
    let row = (y / f64::from(g.line_height)).floor().max(0.) as usize;
    let Row::Source(line) = shared.rows.borrow().row(row)? else {
        return None;
    };
    let shape = g.shaped.get(&line)?;
    let x = point.x - g.bounds.origin.x - px(g.gutter - viewport.x as f32);
    let column = shape.closest_index_for_x(x);
    let column = shared
        .hints
        .borrow()
        .get(&line)
        .map(|d| d.display_to_source(column))
        .unwrap_or(column);
    let doc = shared.highlighted.borrow();
    let text = doc.document.line(line)?;
    let mut column = column.min(text.len());
    while column > 0 && !text.is_char_boundary(column) {
        column -= 1;
    }
    Some(Position::new(line, column))
}
pub(crate) fn anchor(shared: &Shared, position: Position) -> Option<Point<Pixels>> {
    let g = shared.geometry.borrow();
    let row = shared.rows.borrow().source_row(position.line)?;
    let shape = g.shaped.get(&position.line)?;
    let viewport = shared.viewport.get();
    let y = (row as f64 * f64::from(g.line_height) - viewport.y) as f32 + g.line_height * 0.5;
    if y < 0. || y > g.bounds.size.height.as_f32() {
        return None;
    }
    let doc = shared.highlighted.borrow();
    doc.document.offset(position)?;
    let column = shared
        .hints
        .borrow()
        .get(&position.line)
        .map(|d| d.source_to_display(position.column, true))
        .unwrap_or(position.column);
    if column > shape.text.len() {
        return None;
    }
    let anchor = point(
        g.bounds.origin.x
            + px(g.gutter - viewport.x as f32)
            + shape.x_for_index(
                shared
                    .hints
                    .borrow()
                    .get(&position.line)
                    .map(|d| d.source_to_display(position.column, true))
                    .unwrap_or(position.column),
            ),
        g.bounds.origin.y + px(y),
    );
    if anchor.x < g.bounds.origin.x + px(g.gutter)
        || anchor.x > g.bounds.right() - px(g.margin + g.mini)
    {
        return None;
    }
    Some(anchor)
}
fn visit_range_rects(shared: &Shared, range: SourceRange, mut visit: impl FnMut(Bounds<Pixels>)) {
    let g = shared.geometry.borrow();
    let viewport = shared.viewport.get();
    let doc = shared.highlighted.borrow();
    if doc.document.range(range).is_none() {
        return;
    }
    let rows = shared.rows.borrow();
    let hints = shared.hints.borrow();
    for (line, shape) in &g.shaped {
        if *line < range.start.line || *line > range.end.line {
            continue;
        }
        let Some(row) = rows.source_row(*line) else {
            continue;
        };
        let start = if *line == range.start.line {
            range.start.column
        } else {
            0
        };
        let end = if *line == range.end.line {
            range.end.column
        } else {
            doc.document.line(*line).map(str::len).unwrap_or_default()
        };
        let start = hints
            .get(line)
            .map(|d| d.source_to_display(start, true))
            .unwrap_or(start);
        let end = hints
            .get(line)
            .map(|d| d.source_to_display(end, false))
            .unwrap_or(end);
        let x = g.bounds.origin.x + px(g.gutter - viewport.x as f32) + shape.x_for_index(start);
        let end_x = shape.x_for_index(end);
        let right = (x + (end_x - shape.x_for_index(start)).max(px(1.)))
            .min(g.bounds.right() - px(g.margin + g.mini));
        let left = x.max(g.bounds.origin.x + px(g.gutter));
        if right > left {
            visit(Bounds::new(
                point(
                    left,
                    g.bounds.origin.y
                        + px((row as f64 * f64::from(g.line_height) - viewport.y) as f32),
                ),
                size(right - left, px(g.line_height)),
            ));
        }
    }
}
pub(crate) fn paint_overlay(shared: &Rc<Shared>, window: &mut Window, cx: &mut App) {
    let owned_theme = GuiraffeTheme::global(cx).clone();
    let theme = &owned_theme;
    let surfaces = theme.surfaces();
    if let Some(selection) = shared.selection.get() {
        let ink = color(ColorName::Blue, theme).opacity(0.22);
        visit_range_rects(shared, selection, |rect| window.paint_quad(fill(rect, ink)));
    }
    if let Some(symbol) = shared.symbol.borrow().as_ref() {
        let ink = color(ColorName::Yellow, theme).opacity(0.25);
        for range in symbol.occurrences.iter() {
            visit_range_rects(shared, *range, |rect| window.paint_quad(fill(rect, ink)));
        }
    }
    paint_local_traces(shared, &shared.traces.borrow(), theme, window);
    let g = shared.geometry.borrow();
    let options = shared.options.borrow();
    if options.minimap && g.mini > 0. {
        let origin = point(g.bounds.right() - px(g.mini), g.bounds.origin.y);
        let doc = shared.highlighted.borrow();
        let height = g.bounds.size.height.as_f32();
        window.paint_quad(fill(
            Bounds::new(origin, size(px(g.mini), px(height))),
            surfaces.surface.to_hsla(),
        ));
        let total = doc.document.line_count().max(1);
        let ink = surfaces.dimmed.to_hsla().opacity(0.45);
        let (first, last) = (
            g.shaped.keys().min().copied().unwrap_or_default(),
            g.shaped.keys().max().copied().unwrap_or_default(),
        );
        // A file that fits at >= 2px a line is drawn as line strokes, like a miniature;
        // larger files use the bounded density summary.
        let (y, h) = match minimap_line_height(total, height) {
            Some(step) => {
                for line in 0..total {
                    let text = doc.document.line(line).unwrap_or_default();
                    if let Some((x, w)) = minimap_stroke(text, g.mini) {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(origin.x + px(x), origin.y + px(line as f32 * step)),
                                size(px(w), px((step * 0.6).max(1.))),
                            ),
                            ink,
                        ));
                    }
                }
                (
                    first as f32 * step,
                    ((last.saturating_sub(first) + 1) as f32 * step).max(2.),
                )
            }
            None => {
                for (i, density) in doc.document.minimap().iter().enumerate() {
                    if *density > 0. {
                        let y = origin.y + px(i as f32 * height / 256.);
                        window.paint_quad(fill(
                            Bounds::new(
                                point(origin.x, y),
                                size(px(density * g.mini), px((height / 256.).max(1.))),
                            ),
                            ink.opacity(0.35),
                        ));
                    }
                }
                let t = total as f32;
                let y = first as f32 / t * height;
                (y, ((last - first + 1) as f32 / t * height).max(2.))
            }
        };
        let h = h.min((height - y).max(0.));
        let blue = color(ColorName::Blue, theme);
        window.paint_quad(
            fill(
                Bounds::new(point(origin.x, origin.y + px(y)), size(px(g.mini), px(h))),
                blue.opacity(0.08),
            )
            .border_widths(px(1.))
            .border_color(blue.opacity(0.45)),
        );
    }
}
/// Per-line minimap step when every line fits at 2px or more; capped at 4px.
pub(crate) fn minimap_line_height(lines: usize, height: f32) -> Option<f32> {
    let step = height / lines.max(1) as f32;
    (step >= 2.).then_some(step.min(4.))
}
/// Horizontal extent of a line's stroke: indentation offset and trimmed length, both
/// scaled so 120 columns span the minimap.
pub(crate) fn minimap_stroke(text: &str, width: f32) -> Option<(f32, f32)> {
    let trimmed = text.trim_start();
    let len = trimmed.trim_end().chars().count();
    if len == 0 {
        return None;
    }
    let indent = text.len() - trimmed.len();
    let scale = width / 120.;
    let x = (indent.min(120) as f32 * scale).min(width - 1.);
    Some((x, (len as f32 * scale).clamp(1., width - x)))
}
/// Traces within one viewer: a dot on each endpoint token, joined by an arc routed through
/// the gutter so the line never crosses source text. Each trace bulges a little further
/// so several (even crossing) traces stay distinguishable.
fn paint_local_traces(
    shared: &Rc<Shared>,
    traces: &[Trace],
    theme: &GuiraffeTheme,
    window: &mut Window,
) {
    let (edge, gutter, top, bottom) = {
        let g = shared.geometry.borrow();
        (
            g.bounds.origin.x + px(g.gutter),
            g.gutter,
            g.bounds.origin.y,
            g.bounds.bottom(),
        )
    };
    let mut lane = 0usize;
    for trace in traces {
        if trace.from.viewer != shared.id || trace.to.viewer != shared.id {
            continue;
        }
        let (Some(a), Some(b)) = (
            anchor(shared, trace.from.position),
            anchor(shared, trace.to.position),
        ) else {
            continue;
        };
        let c = color(trace.color, theme).opacity(0.8);
        for p in [a, b] {
            window.paint_quad(
                fill(
                    Bounds::new(point(p.x - px(2.5), p.y - px(2.5)), size(px(5.), px(5.))),
                    c,
                )
                .corner_radii(px(2.5)),
            );
        }
        let bulge = px(trace_bulge(lane, gutter));
        lane += 1;
        let x = edge - px(2.);
        let (y0, y1) = (a.y.clamp(top, bottom), b.y.clamp(top, bottom));
        let mut path = PathBuilder::stroke(px(1.5));
        path.move_to(point(x, y0));
        path.cubic_bezier_to(point(x, y1), point(x - bulge, y0), point(x - bulge, y1));
        if let Ok(path) = path.build() {
            window.paint_path(path, c);
        }
        for y in [y0, y1] {
            window.paint_quad(fill(
                Bounds::new(point(x, y - px(0.75)), size(px(2.), px(1.5))),
                c,
            ));
        }
    }
}
/// How far trace `lane` bulges into a gutter of `gutter` px.
pub(crate) fn trace_bulge(lane: usize, gutter: f32) -> f32 {
    (8. + 5. * (lane % 4) as f32).min((gutter * 0.9).max(4.))
}
pub(crate) fn paint_traces(
    viewers: &[Rc<Shared>],
    traces: &[Trace],
    theme: &GuiraffeTheme,
    window: &mut Window,
) {
    for trace in traces {
        let from = viewers.iter().find(|s| s.id == trace.from.viewer);
        let to = viewers.iter().find(|s| s.id == trace.to.viewer);
        let (Some(fs), Some(ts)) = (from, to) else {
            continue;
        };
        let (Some(a), Some(b)) = (
            anchor(fs, trace.from.position),
            anchor(ts, trace.to.position),
        ) else {
            continue;
        };
        let c = color(trace.color, theme).opacity(0.75);
        // Run along the gap below each endpoint's row (not through glyphs) to the facing
        // edge of its viewer, then cross the space between viewers with one curve.
        let edges = |s: &Shared, toward_right: bool| {
            let g = s.geometry.borrow();
            let edge = if toward_right {
                g.bounds.right() - px(g.margin + g.mini)
            } else {
                g.bounds.origin.x + px(g.gutter)
            };
            (edge, px(g.line_height * 0.5))
        };
        let rightward = b.x >= a.x;
        let (ea, ha) = edges(fs, rightward);
        let (eb, hb) = edges(ts, !rightward);
        let (ya, yb) = (a.y + ha, b.y + hb);
        for p in [a, b] {
            window.paint_quad(
                fill(
                    Bounds::new(point(p.x - px(2.5), p.y - px(2.5)), size(px(5.), px(5.))),
                    c,
                )
                .corner_radii(px(2.5)),
            );
        }
        let bend = ((eb - ea).abs() * 0.5).max(px(guiraffe_core::Size::Lg.spacing()));
        let dir = if rightward { 1. } else { -1. };
        let mut path = PathBuilder::stroke(px(1.5));
        path.move_to(a);
        path.line_to(point(a.x, ya));
        path.line_to(point(ea, ya));
        path.cubic_bezier_to(
            point(eb, yb),
            point(ea + bend * dir, ya),
            point(eb - bend * dir, yb),
        );
        path.line_to(point(b.x, yb));
        path.line_to(b);
        if let Ok(path) = path.build() {
            window.paint_path(path, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SHAPE_CACHE_ENTRIES, MAX_SHAPE_CACHE_TEXT_BYTES, MAX_SHAPED_LINE_BYTES, ShapeCache,
    };
    use gpui_kit::{ShapedLine, SharedString};
    use std::rc::Rc;

    #[test]
    fn shape_cache_evicts_by_text_size_as_well_as_entry_count() {
        let mut cache = ShapeCache::default();
        let mut shape = ShapedLine::default();
        shape
            .text
            .clone_from(&SharedString::from("x".repeat(MAX_SHAPED_LINE_BYTES)));
        let shape = Rc::new(shape);
        let capacity = MAX_SHAPE_CACHE_TEXT_BYTES / MAX_SHAPED_LINE_BYTES;
        for row in 0..=capacity {
            cache.insert((0, 0, row, 0, 0), shape.clone());
        }
        assert_eq!(cache.items.len(), capacity);
        assert_eq!(cache.text_bytes, MAX_SHAPE_CACHE_TEXT_BYTES);
        assert!(!cache.items.contains_key(&(0, 0, 0, 0, 0)));

        let empty = Rc::new(ShapedLine::default());
        for row in 1..=MAX_SHAPE_CACHE_ENTRIES {
            cache.insert((0, 0, row, 0, 0), empty.clone());
        }
        assert_eq!(cache.items.len(), MAX_SHAPE_CACHE_ENTRIES);
        assert_eq!(cache.order.len(), cache.items.len());
        assert_eq!(cache.text_bytes, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_shape_cache_keeps_only_the_viewport_and_overscan() {
        for line_count in [10_000, 10_000_000] {
            let map = RowMap::new(line_count, &[], &[]);
            let mut cache = ShapeCache::default();
            cache.retain_viewport_styles(&map, &(100..120), 1, true);
            for line in [100, 110, 119] {
                let mut shape = ShapedLine::default();
                shape.text = "let value = 42;".into();
                cache.insert((0, 1, line, 0, 0), Rc::new(shape));
            }
            cache.retain_viewport_styles(&map, &(119..139), 1, true);
            assert_eq!(
                cache.order.iter().map(|key| key.2).collect::<Vec<_>>(),
                vec![119]
            );
            assert_eq!(cache.text_bytes, "let value = 42;".len());
            cache.retain_viewport_styles(&map, &(9_000..9_020), 1, true);
            assert!(cache.items.is_empty());
            assert!(cache.order.is_empty());
            assert_eq!(cache.text_bytes, 0);
        }
    }
}
