import { FC } from 'react';
import { Select } from '../../ui/Select';
import type { DrawSwitch } from '../../../hooks/useDrawSwitch';
import type { ImageModelRow } from '../../../types/generated/ImageModelRow';
import { componentRoleLabel, imageFamilyLabel } from '../../../utils/imageFamily';

/** The option that stands for the machine's default rule. */
const DEFAULT = 'default';

/** A row as the menu reads it: its name, its family and, when it is, that it is loaded. */
function rowLabel(row: ImageModelRow): string {
  // A family this page does not know is shown as the machine sent it.
  const family = imageFamilyLabel(row.family) ?? row.family;
  return [row.name, family, row.resident && 'loaded', !row.complete && 'incomplete'].filter(Boolean).join(' · ');
}

/** Why a row cannot be chosen: the files it lacks, by role. */
function missingTitle(row: ImageModelRow): string | undefined {
  if (row.complete) return undefined;
  // A role this page does not know is shown as the machine sent it.
  return `Missing: ${row.missing.map((role) => componentRoleLabel(role) ?? role).join(', ')}`;
}

/**
 * The image model the open chat's pictures are drawn with, beside the Draw
 * button: the machine's default rule, naming what it draws with, then each
 * image model in the machine's order. One that lacks a file is listed and
 * cannot be chosen; its title says what it lacks. Nothing is drawn where the
 * machine lists fewer than two and the chat has chosen none, since there is
 * then nothing to choose; a chat that has chosen keeps the menu however
 * short the list has become, so the choice can always be taken back.
 *
 * It stays usable while the button is greyed: a choice that no longer draws
 * is changed here. A choice the machine no longer lists keeps an option of
 * its own, so the menu never shows another model as chosen.
 */
export const ImageModelMenu: FC<{ draw: DrawSwitch }> = ({ draw }) => {
  const { models, picked, pick } = draw;
  if (models.length < 2 && picked === null) return null;
  const standing = models.find((row) => row.default);
  const unlisted = picked !== null && !models.some((row) => row.id === picked);
  return (
    <span className="w-[150px]">
      <Select
        aria-label="Image model"
        title="The image model this chat's pictures are drawn with. Choosing one presses Draw for the next message."
        size="sm"
        value={picked ?? DEFAULT}
        onChange={(event) => pick(event.target.value === DEFAULT ? null : Number(event.target.value))}
        className="bg-transparent border-transparent text-text-secondary truncate"
      >
        <option value={DEFAULT}>{standing ? `Default (${standing.name})` : 'Default'}</option>
        {unlisted && <option value={picked}>Model {picked} (no longer listed)</option>}
        {models.map((row) => (
          <option key={row.id} value={row.id} disabled={!row.complete} title={missingTitle(row)}>
            {rowLabel(row)}
          </option>
        ))}
      </Select>
    </span>
  );
};
