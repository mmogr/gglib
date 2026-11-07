/**
 * A ▸ line that opens what it holds in place: Endpoint's "What gglib does
 * to requests" and Tools' "Agent limits", the two that hold what was
 * advanced. Closed when the section opens.
 *
 * @module places/settings/Disclosure
 */

import { useState, type FC, type ReactNode } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';

export const Disclosure: FC<{ label: string; children: ReactNode }> = ({ label, children }) => {
  const [open, setOpen] = useState(false);
  return (
    <div className="flex flex-col gap-md">
      <Button
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
