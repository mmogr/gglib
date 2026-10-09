import { FC } from 'react';
import { Palette } from 'lucide-react';
import type { HfImagePreview } from '../../types';
import { componentRoleLabel, imageFamilyLabel } from '../../utils/imageFamily';
import { formatBytes } from '../../utils/format';
import { Icon } from '../ui/Icon';

interface CompanionNoteProps {
  /** The daemon's reading of the repository as an image model, with the companions its download fetches */
  image: HfImagePreview;
}

function fileName(path: string): string {
  return path.split('/').pop() || path;
}

/**
 * The files an image model's download fetches beside its weights, before
 * anything is fetched: the family read from the weights' head, each
 * companion's role, file name, repository and size, "already here" for one
 * a download will not fetch again, and the bytes the companions add.
 *
 * The companions are the family's, so the note is the same whichever
 * quantization is selected.
 */
export const CompanionNote: FC<CompanionNoteProps> = ({ image }) => {
  const family = imageFamilyLabel(image.family);

  return (
    <div data-testid="companion-note" className="flex flex-col gap-sm px-base py-md border border-border rounded-lg bg-surface">
      <div className="flex flex-wrap items-center gap-sm text-sm text-text">
        <span className="text-text-secondary" aria-hidden>
          <Icon icon={Palette} size={14} />
        </span>
        <span>An image model of the {family} family.</span>
      </div>
      {image.companions.length === 0 ? (
        <p className="m-0 text-xs leading-relaxed text-text-secondary">Its weights file is all it needs.</p>
      ) : (
        <>
          <p className="m-0 text-xs leading-relaxed text-text-secondary">A download also fetches these files:</p>
          <ul className="m-0 p-0 list-none flex flex-col gap-xs">
            {image.companions.map((companion) => (
              <li
                key={companion.role}
                data-testid={`companion-${companion.role}`}
                className="flex flex-wrap items-baseline gap-sm text-xs text-text"
              >
                <span className="text-text-secondary">{componentRoleLabel(companion.role)}</span>
                <span className="font-mono break-all" title={`${companion.repo}/${companion.file_path}`}>
                  {fileName(companion.file_path)}
                </span>
                <span className="text-text-muted break-all">{companion.repo}</span>
                <span className="text-text-secondary tabular-nums">{formatBytes(companion.size_bytes)}</span>
                {companion.present && <span className="text-text-muted">already here</span>}
              </li>
            ))}
          </ul>
          <p data-testid="companion-total" className="m-0 text-xs leading-relaxed text-text-secondary">
            {image.fetch_bytes === 0
              ? 'Every one is already here, so the download fetches the weights alone.'
              : `Beside the weights, the download fetches ${formatBytes(image.fetch_bytes)}.`}
          </p>
        </>
      )}
    </div>
  );
};
