/**
 * An engine row's ▸ Details: how the install got there, as its record says
 * (the release or the commit it was built from, the download or the
 * acceleration, the date), the binary and what it reports, the model
 * running on it, and where it stands against its pin (`standing`, the
 * words `gglib config llama|sd status` prints). Only what the engine's
 * status has: a line it leaves empty is not drawn.
 *
 * @module places/settings/EngineDetails
 */

import type { FC } from 'react';
import type { EngineStatus } from '../../types/setup';
import { LabelledValue } from './LabelledValue';

/** How the install got there, as its record says, or why there is no record to say. */
const Recorded: FC<{ status: EngineStatus }> = ({ status }) => {
  if (!status.install) {
    return (
      <LabelledValue
        label="Install record"
        value={status.recordError ?? 'none, installed outside gglib'}
        mono={false}
      />
    );
  }
  const { built, release, platform, date } = status.install;
  return (
    <>
      <LabelledValue label={built ? 'Built from' : 'Release'} value={release} />
      <LabelledValue label={built ? 'Acceleration' : 'Download'} value={platform} mono={false} />
      <LabelledValue label={built ? 'Built' : 'Downloaded'} value={date.split('T')[0]} />
    </>
  );
};

export const EngineDetails: FC<{ status: EngineStatus }> = ({ status }) => (
  <div className="flex flex-col gap-xs">
    <Recorded status={status} />
    <LabelledValue label="Binary" value={status.binaryPath} />
    {status.reports && <LabelledValue label="Binary reports" value={status.reports} />}
    {status.serving && <LabelledValue label="Serving" value={status.serving} mono={false} />}
    {status.standing && <p className="m-0 text-xs text-text-secondary">{status.standing}</p>}
  </div>
);
