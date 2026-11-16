import { FC } from 'react';
import { HF_REPO, fileName as nameOf } from '../../services/hostRegistry';
import { HfQuantization } from '../../types';
import { formatSize } from '../../utils/format';

/**
 * The projector a download of the selected quantization brings, in one
 * line: "Brings mmproj-F16.gguf (0.8 GB) for images". Its cost is said on
 * hover, following llama-server: with a projector loaded it switches off
 * context shift and `--cache-reuse`, and still reuses a prompt's common
 * prefix. Nothing for a quantization whose repository has no projector.
 */
export const ProjectorNote: FC<{ quantization: HfQuantization }> = ({ quantization }) => {
  const projector = quantization.projector;
  if (!projector) return null;

  const fileName = nameOf(projector.file_path, HF_REPO);
  const size = formatSize(projector.size_bytes);
  return (
    <p
      data-testid="projector-note"
      className="m-0 text-xs text-text-secondary"
      title={`It adds about ${size} in memory. With a projector, llama-server reuses only the unchanged start of a prompt: context shift and cache reuse by shifting are off.`}
    >
      {'Brings '}
      <span className="font-mono text-text">{fileName}</span>
      {` (${size}) for images`}
    </p>
  );
};
