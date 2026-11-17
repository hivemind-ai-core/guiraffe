import { memo, useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from 'react';
import { Group, Text, TextInput } from '@mantine/core';
import {
  columnFilteringFeature,
  createColumnHelper,
  createFilteredRowModel,
  createSortedRowModel,
  filterFn_includesString,
  rowSelectionFeature,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  tableFeatures,
  useTable,
  type ColumnFiltersState,
  type RowSelectionState,
  type SortingState,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';
import { generateRows, type Row } from '../../shared/data';
import { register } from '../benchHooks';

const ROW_HEIGHT = 32;

const features = tableFeatures({
  columnFilteringFeature,
  rowSortingFeature,
  rowSelectionFeature,
  filteredRowModel: createFilteredRowModel(),
  sortedRowModel: createSortedRowModel(),
  filterFns: { includesString: filterFn_includesString },
  sortFns: { alphanumeric: sortFn_alphanumeric, basic: sortFn_basic },
});

const helper = createColumnHelper<typeof features, Row>();

interface ColMeta {
  width: number;
  num?: boolean;
}

const COL_META: Record<string, ColMeta> = {
  id: { width: 70, num: true },
  name: { width: 150 },
  email: { width: 230 },
  city: { width: 100 },
  country: { width: 110 },
  department: { width: 120 },
  status: { width: 80 },
  amount: { width: 100, num: true },
  quantity: { width: 80, num: true },
  score: { width: 70, num: true },
  active: { width: 60 },
  created: { width: 100 },
};
const TOTAL_WIDTH = Object.values(COL_META).reduce((a, m) => a + m.width, 0);

const text = { sortFn: 'alphanumeric' as const };
const num = { sortFn: 'basic' as const };

// Cells are rendered by the memoised GridRow below (formatting lives there).
const columns = helper.columns([
  helper.accessor('id', { header: 'ID', ...num }),
  helper.accessor('name', { header: 'Name', ...text, filterFn: 'includesString' }),
  helper.accessor('email', { header: 'Email', ...text }),
  helper.accessor('city', { header: 'City', ...text }),
  helper.accessor('country', { header: 'Country', ...text }),
  helper.accessor('department', { header: 'Department', ...text }),
  helper.accessor('status', { header: 'Status', ...text }),
  helper.accessor('amount', { header: 'Amount', ...num }),
  helper.accessor('quantity', { header: 'Quantity', ...num }),
  helper.accessor('score', { header: 'Score', ...num }),
  helper.accessor('active', { header: 'Active', ...num }),
  helper.accessor('created', { header: 'Created', ...text }),
]);

// Generated when the grid page is first opened, then kept.
let cachedRows: Row[] | null = null;
function getRows(): Row[] {
  cachedRows ??= generateRows();
  return cachedRows;
}

const getRowId = (row: Row) => String(row.id);

export default function GridPage() {
  const [data] = useState(getRows);
  const [sorting, setSorting] = useState<SortingState>([]);
  const [filter, setFilter] = useState('');
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  const anchor = useRef<number | null>(null);

  const columnFilters = useMemo<ColumnFiltersState>(
    () => (filter ? [{ id: 'name', value: filter }] : []),
    [filter],
  );

  const table = useTable({
    features,
    columns,
    data,
    getRowId,
    state: { sorting, columnFilters, rowSelection },
    onSortingChange: setSorting,
    onRowSelectionChange: setRowSelection,
    sortDescFirst: false,
    enableMultiSort: true,
  });

  const rows = table.getRowModel().rows;
  const scrollRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  // Click: select one; Ctrl/Cmd+click: toggle; Shift+click: range from the anchor.
  const onRowClick = useCallback(
    (index: number, e: MouseEvent) => {
      const id = rows[index]?.id;
      if (id === undefined) return;
      if (e.shiftKey && anchor.current !== null) {
        const [a, b] = anchor.current < index ? [anchor.current, index] : [index, anchor.current];
        const next: RowSelectionState = {};
        for (let i = a; i <= b; i++) next[rows[i].id] = true;
        setRowSelection(next);
        return;
      }
      anchor.current = index;
      if (e.ctrlKey || e.metaKey) {
        setRowSelection((prev) => {
          const next = { ...prev };
          if (next[id]) delete next[id];
          else next[id] = true;
          return next;
        });
      } else {
        setRowSelection({ [id]: true });
      }
    },
    [rows],
  );

  const rowsRef = useRef(rows.length);
  rowsRef.current = rows.length;
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    register('grid', {
      scrollEl: el,
      setSorting,
      setFilter,
      rowCount: () => rowsRef.current,
    });
    return () => register('grid', undefined);
  }, []);

  const selectedCount = Object.keys(rowSelection).length;

  return (
    <div className="page">
      <Group justify="space-between">
        <TextInput
          placeholder="Filter by name…"
          value={filter}
          onChange={(e) => setFilter(e.currentTarget.value)}
          w={280}
        />
        <Text size="sm" c="dimmed">
          {rows.length.toLocaleString('en-US')} rows · {selectedCount.toLocaleString('en-US')} selected
        </Text>
      </Group>
      <div className="grid-scroll" ref={scrollRef}>
        <div className="grid-header" style={{ width: TOTAL_WIDTH }}>
          {table.getHeaderGroups()[0].headers.map((header) => {
            const meta = COL_META[header.column.id];
            const dir = header.column.getIsSorted();
            const idx = sorting.length > 1 ? header.column.getSortIndex() : -1;
            return (
              <div
                key={header.id}
                className={meta.num ? 'grid-hcell num' : 'grid-hcell'}
                style={{ width: meta.width }}
                onClick={header.column.getToggleSortingHandler()}
              >
                <table.FlexRender header={header} />
                {dir === 'asc' ? ' ▲' : dir === 'desc' ? ' ▼' : ''}
                {idx >= 0 ? <sup>{idx + 1}</sup> : null}
              </div>
            );
          })}
        </div>
        <div style={{ height: virtualizer.getTotalSize(), width: TOTAL_WIDTH, position: 'relative' }}>
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index];
            return (
              <GridRow
                key={row.id}
                row={row.original}
                index={item.index}
                start={item.start}
                selected={rowSelection[row.id] === true}
                onClick={onRowClick}
              />
            );
          })}
        </div>
      </div>
    </div>
  );
}

const GridRow = memo(function GridRow({
  row,
  index,
  start,
  selected,
  onClick,
}: {
  row: Row;
  index: number;
  start: number;
  selected: boolean;
  onClick: (index: number, e: MouseEvent) => void;
}) {
  return (
    <div
      className="grid-row"
      data-selected={selected}
      style={{ transform: `translateY(${start}px)`, width: TOTAL_WIDTH }}
      onClick={(e) => onClick(index, e)}
    >
      <Cell w={COL_META.id.width} num>
        {row.id}
      </Cell>
      <Cell w={COL_META.name.width}>{row.name}</Cell>
      <Cell w={COL_META.email.width}>{row.email}</Cell>
      <Cell w={COL_META.city.width}>{row.city}</Cell>
      <Cell w={COL_META.country.width}>{row.country}</Cell>
      <Cell w={COL_META.department.width}>{row.department}</Cell>
      <Cell w={COL_META.status.width}>{row.status}</Cell>
      <Cell w={COL_META.amount.width} num>
        ${row.amount.toFixed(2)}
      </Cell>
      <Cell w={COL_META.quantity.width} num>
        {row.quantity}
      </Cell>
      <Cell w={COL_META.score.width} num>
        {row.score.toFixed(2)}
      </Cell>
      <Cell w={COL_META.active.width}>{row.active ? 'yes' : 'no'}</Cell>
      <Cell w={COL_META.created.width}>{row.created}</Cell>
    </div>
  );
});

function Cell({ w, num, children }: { w: number; num?: boolean; children: React.ReactNode }) {
  return (
    <div className={num ? 'grid-cell num' : 'grid-cell'} style={{ width: w }}>
      {children}
    </div>
  );
}
