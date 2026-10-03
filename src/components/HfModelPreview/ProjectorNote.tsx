import { FC } from 'react';
import { Eye } from 'lucide-react';
import { HfQuantization } from '../../types';
import { formatBytes } from '../../utils/format';
import { Icon } from '../ui/Icon';

interface ProjectorNoteProps {
  /** The selected quantization, which carries the projector its download fetches */
  quantization: HfQuantization;
}

/**
 * The projector that comes with a download of one quantization: its file
 * name, its size, and what it costs. Nothing for a quantization whose
 * repository has no projector.
 *
 * The cost line follows llama-server: with a projector loaded it switches off
 * context shift and `--cache-reuse`, and still reuses a prompt's common
 * prefix.
 */
export const ProjectorNote: FC<ProjectorNoteProps> = ({ quantization }) => {
  const projector = quantization.projector;
  if (!projector) return null;

  const fileName = projector.file_path.split('/').pop() || projector.file_path;
  const size = formatBytes(projector.size_bytes);

  return (
    <div data-testid="projector-note" className="flex flex-col gap-xs px-base py-md border border-border rounded-lg bg-surface">
      <div className="flex flex-wrap items-center gap-sm text-sm text-text">
        <span className="text-text-secondary" aria-hidden>
          <Icon icon={Eye} size={14} />
        </span>
        <span>{quantization.name} comes with a projector, so the model reads images:</span>
        <span className="font-mono text-xs break-all" title={projector.file_path}>{fileName}</span>
        <span className="text-text-secondary">{size}</span>
      </div>
      <p className="m-0 text-xs leading-relaxed text-text-secondary">
        It adds about {size} in memory. With a projector, llama-server reuses only the unchanged start of a prompt: context shift and cache reuse by shifting are off.
      </p>
    </div>
  );
};
