/**
 * A ▸ line that opens what it holds in place: Endpoint's "What gglib does
 * to requests" and Tools' "Agent limits", the two that hold what was
 * advanced, and the tool server sheet's "More". Closed when the section
 * opens, unless `defaultOpen`; a button that never submits the form it is in.
 *
 * @module places/settings/Disclosure
 */

import { useState, type FC, type ReactNode } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';

export const Disclosure: FC<{ label: string; children: ReactNode; defaultOpen?: boolean }> = ({
  label,
  children,
  defaultOpen = false,
}) => {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="flex flex-col gap-md">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="self-start px-0 font-semibold text-text hover:bg-transparent"
        aria-expanded={open}
        leftIcon={<Icon icon={open ? ChevronDown : ChevronRight} size={14} />}
        onClick={() => setOpen((o) => !o)}
      >
        {label}
      </Button>
      {open && <div className="flex flex-col gap-lg pl-lg">{children}</div>}
    </div>
  );
};
