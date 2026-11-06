/**
 * The library's head: its title, `Sort ▾` and `+ ▾`, and the search field
 * under them. Sort orders each group on this screen (`groups.ts`); `+`
 * finds a model on Hugging Face, adds a GGUF file, or adds a tool server in
 * Settings, each by its address.
 *
 * @module places/library/LibraryHeader
 */

import type { FC } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { Plus, Search } from 'lucide-react';
import { Icon } from '../../components/ui/Icon';
import { Input } from '../../components/ui/Input';
import { paths, settingsAt } from '../../app/paths';
import type { LibrarySort, SortKey } from './groups';
import { Menu } from './Menu';

const SORTS: { key: SortKey; label: string }[] = [
  { key: 'name', label: 'Name' },
  { key: 'added', label: 'Recently added' },
  { key: 'size', label: 'Size' },
  { key: 'speed', label: 'Measured speed' },
];

interface LibraryHeaderProps {
  sort: LibrarySort;
  onSort: (sort: LibrarySort) => void;
  /** The search, or `null` where no search is drawn (the Add pane). */
  query: string | null;
  onQuery: (query: string) => void;
}

export const LibraryHeader: FC<LibraryHeaderProps> = ({ sort, onSort, query, onQuery }) => {
  const location = useLocation();
  const navigate = useNavigate();
  return (
    <div className="flex shrink-0 flex-col gap-sm px-md pb-sm pt-md">
      <div className="flex items-center gap-sm">
        <h2 className="m-0 flex-1 text-base font-semibold text-text">Library</h2>
        <Menu
          label="Sort"
          items={[
            ...SORTS.map(({ key, label }) => ({
              label,
              checked: sort.key === key,
              onPick: () => onSort({ ...sort, key }),
            })),
            { label: 'Ascending', checked: sort.ascending, gapBefore: true, onPick: () => onSort({ ...sort, ascending: true }) },
            { label: 'Descending', checked: !sort.ascending, onPick: () => onSort({ ...sort, ascending: false }) },
          ]}
        />
        <Menu
          label={<Icon icon={Plus} size={14} />}
          name="Add"
          items={[
            { label: 'Find on Hugging Face', onPick: () => navigate(paths.libraryAdd('browse')) },
            { label: 'Add a GGUF file…', onPick: () => navigate(paths.libraryAdd('add')) },
            { label: 'Add a tool server…', onPick: () => navigate(settingsAt(location, 'mcp'), { replace: true }) },
          ]}
        />
      </div>
      {query !== null && (
        <Input
          type="search"
          size="sm"
          aria-label="Search your models"
          placeholder="Search your models"
          leftIcon={<Icon icon={Search} size={14} />}
          value={query}
          onChange={(e) => onQuery(e.target.value)}
        />
      )}
    </div>
  );
};
