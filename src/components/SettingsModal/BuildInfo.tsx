import { FC, useEffect, useState } from 'react';
import { Stack } from '../primitives';
import { getTransport } from '../../services/transport';
import { LabelledValue } from './LabelledValue';
import type { VersionDto } from '../../types/generated/VersionDto';

/**
 * Which build of gglib the daemon is running.
 *
 * Read from the daemon rather than from `package.json`, so a GUI talking to a
 * daemon left over from an older install reports that daemon's build — the
 * same string `gglib --version` prints on the terminal, from the same
 * constants in `gglib-build-info`.
 *
 * Renders nothing until the daemon answers, and nothing at all if it does not:
 * this is provenance for a bug report, not something worth an error banner in
 * a settings panel.
 */
export const BuildInfo: FC = () => {
  const [version, setVersion] = useState<VersionDto | null>(null);

  useEffect(() => {
    let cancelled = false;
    getTransport()
      .getVersion()
      .then((v) => {
        if (!cancelled) setVersion(v);
      })
      .catch(() => {
        // Left null — see the note above.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!version) return null;

  return (
    <Stack gap="xs">
      <LabelledValue label="gglib" value={version.display} />
      {version.fingerprint !== version.sha && (
        <LabelledValue label="Working tree" value="modified since this build" mono={false} />
      )}
    </Stack>
  );
};
