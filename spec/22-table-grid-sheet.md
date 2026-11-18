# Table vs DataGrid vs Sheet: which one?

**Rule of thumb:** columns with app-defined meaning → **DataGrid**. User decides what goes where →
**Sheet**. A fixed display of a little data → **Table**.

| | Table | DataGrid | Sheet |
|---|---|---|---|
| Unit | a row you read | a **record** (row = entity, columns = typed fields) | a **cell** (free-form 2D) |
| Use when | small fixed data in a page: settings, comparisons, ≤ a few hundred rows | browse/sort/filter/edit many records of one kind: orders, users, logs | users compute and lay out data: budgets, models, cleaning imports, ad-hoc analysis |
| Size | no virtualization | millions of rows, virtualized | large sparse grids, virtualized both axes |
| Columns | fixed, styled | app-defined: type, formatter, validator, pin, sort, filter | generic A, B, C…; any cell holds anything |
| Editing | none | **record editing** (below) | **spreadsheet editing** (below) |
| Crate | Kit `Table` (themed) | `guiraffe-datagrid` | `guiraffe-sheet` (0.2), sharing the grid's virtualization, selection, clipboard and undo model |

## DataGrid editing (record editing)
Type-to-edit; F2/Enter edit in place; Enter/Tab commit-and-move (Shift reverses), arrows commit
and move, Esc cancels; per-column validation with veto; Shift+arrows / Shift+click / drag select a
single rectangular range; copy and paste ranges as TSV (paste a column of values from another app
into a column, clipped to editable columns, each cell validated, rejections summarised); Delete
clears editable cells; undo/redo with paste as one step. **Not in the grid:** formulas, fill
handle/series, multiple ranges, Ctrl+D/R, insert/delete rows and columns by the user, frozen panes
beyond pinned columns.

## Sheet editing (spreadsheet editing)
Everything above, plus formulas (pluggable engine, A1 references, recalculation), fill handle and
series, Ctrl+D/Ctrl+R, multiple ranges (Ctrl+drag), Ctrl+arrow jumps to data edges and
Ctrl+Shift+arrow extends, Ctrl+Space/Shift+Space select column/row, insert/delete rows and
columns, frozen panes, cell formatting, marching-ants copy marquee, HTML clipboard.

## Shared model
Selection (cursor + ranges), clipboard (TSV/HTML encode/decode, paste tiling), undo transactions
and keyboard maps live in one module used by both, so overlapping behaviour is identical.
