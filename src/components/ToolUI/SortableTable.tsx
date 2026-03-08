import React from 'react';
import { ChevronDown, ChevronUp, ChevronsUpDown } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';

const MAX_ROWS = 200;

export interface SortableTableProps {
  rows: Record<string, unknown>[];
  columns?: string[];
}

type SortDir = 'asc' | 'desc';

const SortIcon: React.FC<{ col: string; sortKey: string | null; sortDir: SortDir }> = ({
  col,
  sortKey,
  sortDir,
}) => {
  if (sortKey !== col) return <Icon icon={ChevronsUpDown} size={11} className="opacity-40" />;
  return <Icon icon={sortDir === 'asc' ? ChevronUp : ChevronDown} size={11} />;
};

function compareValues(a: unknown, b: unknown, dir: SortDir): number {
  // Both numeric → numericcompare
  if (typeof a === 'number' && typeof b === 'number') {
    return dir === 'asc' ? a - b : b - a;
  }
  const sa = a === null || a === undefined ? '' : String(a);
  const sb = b === null || b === undefined ? '' : String(b);
  const cmp = sa.localeCompare(sb, undefined, { numeric: true, sensitivity: 'base' });
  return dir === 'asc' ? cmp : -cmp;
}

function renderCell(value: unknown): React.ReactNode {
  if (value === null || value === undefined) {
    return <span className="text-text-muted italic">—</span>;
  }
  if (typeof value === 'boolean') {
    return <span className="font-mono text-[11px]">{String(value)}</span>;
  }
  if (typeof value === 'object') {
    try {
      return (
        <span className="font-mono text-[11px] text-text-secondary">
          {JSON.stringify(value)}
        </span>
      );
    } catch {
      return <span className="text-text-muted italic">[object]</span>;
    }
  }
  // Check if it looks like a URL
  if (typeof value === 'string' && /^https?:\/\//.test(value)) {
    return (
      <a
        href={value}
        target="_blank"
        rel="noopener noreferrer"
        className="text-primary underline break-all"
        onClick={(e) => e.stopPropagation()}
      >
        {value}
      </a>
    );
  }
  return String(value);
}

/**
 * Sortable data table for tool results that are arrays of objects.
 * Columns are inferred from the first row if not explicitly provided.
 * Supports click-to-sort column headers with lexicographic/numeric comparison.
 * Caps rendering at 200 rows.
 */
export const SortableTable: React.FC<SortableTableProps> = ({ rows, columns: columnsProp }) => {
  const [sortKey, setSortKey] = React.useState<string | null>(null);
  const [sortDir, setSortDir] = React.useState<SortDir>('asc');

  const columns = React.useMemo(
    () => columnsProp ?? Object.keys(rows[0] ?? {}),
    [columnsProp, rows],
  );

  const sortedRows = React.useMemo(() => {
    if (!sortKey) return rows;
    return [...rows].sort((a, b) => compareValues(a[sortKey], b[sortKey], sortDir));
  }, [rows, sortKey, sortDir]);

  const visibleRows = sortedRows.slice(0, MAX_ROWS);
  const truncated = rows.length > MAX_ROWS;

  const handleSort = (col: string) => {
    if (sortKey === col) {
      setSortDir((prev) => (prev === 'asc' ? 'desc' : 'asc'));
    } else {
      setSortKey(col);
      setSortDir('asc');
    }
  };

  if (columns.length === 0) return null;

  return (
    <div className="overflow-x-auto rounded-lg border border-border my-1 text-[12px]">
      <table className="border-collapse w-full">
        <thead>
          <tr className="bg-background-tertiary">
            {columns.map((col) => (
              <th
                key={col}
                onClick={() => handleSort(col)}
                className={cn(
                  'border-b border-border py-1.5 px-2.5 text-left font-semibold text-text-secondary',
                  'cursor-pointer select-none whitespace-nowrap',
                  'hover:text-text hover:bg-background-secondary transition-colors duration-100',
                  sortKey === col && 'text-text',
                )}
              >
                <span className="inline-flex items-center gap-1">
                  {col}
                  <SortIcon col={col} sortKey={sortKey} sortDir={sortDir} />
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {visibleRows.map((row, rowIdx) => (
            <tr
              key={rowIdx}
              className="border-b border-border last:border-b-0 hover:bg-background-secondary transition-colors duration-75"
            >
              {columns.map((col) => (
                <td
                  key={col}
                  className="py-1.5 px-2.5 text-left text-text align-top border-r border-border last:border-r-0"
                >
                  {renderCell(row[col])}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>

      {truncated && (
        <p className="px-3 py-1.5 text-[11px] text-text-muted border-t border-border bg-background-secondary">
          Showing {MAX_ROWS} of {rows.length} rows.
        </p>
      )}
    </div>
  );
};

export default SortableTable;
