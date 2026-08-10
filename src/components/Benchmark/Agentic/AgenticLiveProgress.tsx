import type { FC } from 'react';
import { Check, X } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { cn } from '../../../utils/cn';
import type { EvalArm } from '../../../types/benchmark';

/** How the CLI banner names each arm — kept identical so the two agree. */
export const ARM_LABELS: Record<EvalArm, string> = {
  raw: 'raw (pipeline bypassed)',
  gglib: 'gglib (full pipeline)',
  raw_replicate: 'raw again (A/A, disjoint seeds)',
  control: 'control (sampling deliberately broken)',
};

export interface ArmProgress {
  arm: EvalArm;
  total: number;
  done: number;
  passed: number;
}

export interface TaskLogEntry {
  arm: EvalArm;
  taskId: string;
  passed: boolean;
}

interface AgenticLiveProgressProps {
  arms: ArmProgress[];
  currentArm: EvalArm | null;
  taskLog: TaskLogEntry[];
}

/** Per-arm progress rows plus the scrolling pass/fail task log. */
export const AgenticLiveProgress: FC<AgenticLiveProgressProps> = ({
  arms,
  currentArm,
  taskLog,
}) => (
  <div className="flex flex-col gap-sm">
    <div className="flex flex-col gap-xs">
      {arms.map((a) => (
        <div key={a.arm} className="flex items-center justify-between gap-md">
          <span
            className={cn(
              'text-sm',
              a.arm === currentArm ? 'text-text font-medium' : 'text-text-muted',
            )}
          >
            {ARM_LABELS[a.arm]}
          </span>
          <span className="text-sm text-text-secondary font-mono tabular-nums shrink-0">
            {a.done}/{a.total} · {a.passed} passed
          </span>
        </div>
      ))}
    </div>

    {taskLog.length > 0 && (
      <div className="flex flex-col gap-xs max-h-[220px] overflow-y-auto bg-surface rounded-md p-sm">
        {taskLog.map((entry, i) => (
          <div key={i} className="flex items-center gap-sm text-xs">
            <span className={entry.passed ? 'text-success' : 'text-danger'} aria-hidden>
              <Icon icon={entry.passed ? Check : X} size={12} />
            </span>
            <span className="sr-only">{entry.passed ? 'passed' : 'failed'}</span>
            <span className="text-text-secondary font-mono">{entry.taskId}</span>
            <span className="text-text-muted ml-auto shrink-0">{entry.arm}</span>
          </div>
        ))}
      </div>
    )}
  </div>
);
