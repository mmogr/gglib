import { FC, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { cn } from '../../../utils/cn';
import { isUnstable, passCounts } from './verdicts';
import type { AgenticEvalReport } from '../../../types/benchmark';

const Td: FC<{ children: React.ReactNode; className?: string }> = ({ children, className }) => (
  <td className={cn('px-md py-xs text-sm text-text font-mono tabular-nums', className)}>{children}</td>
);
const Th: FC<{ children?: React.ReactNode }> = ({ children }) => (
  <th className="px-md py-xs text-left text-xs font-medium text-text-muted">{children}</th>
);

/** Collapsible per-task drill-down — detail the CLI does not print. */
export const AgenticTaskDrilldown: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const [open, setOpen] = useState(false);
  if (report.tasks.length === 0) return null;

  return (
    <section className="flex flex-col gap-xs">
      <Button
        variant="ghost"
        size="sm"
        className="self-start"
        aria-expanded={open}
        aria-controls="agentic-task-table"
        onClick={() => setOpen(!open)}
        leftIcon={
          <Icon icon={ChevronRight} size={14} className={cn('transition-transform', open && 'rotate-90')} />
        }
      >
        Per-task results ({report.tasks.length})
      </Button>
      {open && (
        <div id="agentic-task-table" className="overflow-x-auto bg-surface-elevated rounded-md">
          <table className="w-full text-xs border-collapse">
            <thead>
              <tr className="border-b border-border-light">
                <Th>Task</Th>
                <Th>Category</Th>
                <Th>raw</Th>
                <Th>gglib</Th>
                <Th />
              </tr>
            </thead>
            <tbody>
              {report.tasks.map((t) => {
                const [rawPassed, gglibPassed] = passCounts(t);
                return (
                  <tr key={t.task_id} className="border-b border-border-light last:border-b-0">
                    <Td className="text-text-secondary">{t.task_id}</Td>
                    <td className="px-md py-xs text-xs text-text-muted">{t.category}</td>
                    <Td>{`${rawPassed}/${t.raw.length}`}</Td>
                    <Td>{`${gglibPassed}/${t.gglib.length}`}</Td>
                    <td className="px-md py-xs text-xs text-warning">
                      {isUnstable(t) ? 'unstable' : ''}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
};
