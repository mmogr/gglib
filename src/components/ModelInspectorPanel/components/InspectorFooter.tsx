import { FC } from 'react';
import { BarChart2, Check, Pencil, Rocket, Square, Trash2, X } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';

interface InspectorFooterProps {
  isRunning: boolean;
  isEditMode: boolean;
  onToggleServer: () => void;
  onEdit: () => void;
  onSave: () => void;
  onCancel: () => void;
  onDelete: () => void;
  onBenchmark?: () => void;
}

/**
 * Sticky action bar pinned to the bottom of the inspector.
 *
 * Lives outside the scroll container so the primary verb (Start/Stop
 * Endpoint) is reachable without scrolling past metadata and tags. The
 * primary keeps its intrinsic width rather than stretching edge to edge.
 */
export const InspectorFooter: FC<InspectorFooterProps> = ({
  isRunning,
  isEditMode,
  onToggleServer,
  onEdit,
  onSave,
  onCancel,
  onDelete,
  onBenchmark,
}) => (
  <div className="flex items-center justify-between flex-wrap gap-md p-base border-t border-border bg-background shrink-0">
    {isEditMode ? (
      <>
        <Button variant="primary" onClick={onSave} leftIcon={<Icon icon={Check} size={14} />}>
          Save
        </Button>
        <Button variant="secondary" onClick={onCancel} leftIcon={<Icon icon={X} size={14} />}>
          Cancel
        </Button>
      </>
    ) : (
      <>
        <Button
          variant={isRunning ? 'danger' : 'primary'}
          size="lg"
          onClick={onToggleServer}
          leftIcon={<Icon icon={isRunning ? Square : Rocket} size={16} />}
        >
          {isRunning ? 'Stop Endpoint' : 'Start Endpoint'}
        </Button>

        <div className="flex items-center gap-sm">
          <Button variant="secondary" onClick={onEdit} leftIcon={<Icon icon={Pencil} size={14} />}>
            Edit
          </Button>
          {onBenchmark && (
            <Button
              variant="secondary"
              onClick={onBenchmark}
              leftIcon={<Icon icon={BarChart2} size={14} />}
            >
              Benchmark
            </Button>
          )}
          <Button
            variant="danger"
            onClick={onDelete}
            leftIcon={<Icon icon={Trash2} size={14} />}
          >
            Delete
          </Button>
        </div>
      </>
    )}
  </div>
);
