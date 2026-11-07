import { FC, useEffect, useState } from 'react';
import { getTransport } from '../../services/transport';
import type { VersionDto } from '../../types/generated/VersionDto';

/**
 * Which build of gglib the daemon is running: "gglib 0.31.0 (abc1234)",
 * and "Working tree modified since this build" under it when the build's
 * fingerprint carries the dirty mark its commit does not.
 *
 * Read from the daemon rather than from `package.json`, so a window talking
 * to a daemon left over from an older install reports that daemon's build:
 * the same string `gglib --version` prints, from the same constants in
 * `gglib-build-info`.
 *
 * Renders nothing until the daemon answers, and nothing at all if it does
 * not: this is provenance for a bug report, not worth an error line.
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
        // Left null: see the note above.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!version) return null;

  return (
    <div className="flex flex-col gap-xs">
      <p className="m-0 text-sm text-text tabular-nums">gglib {version.display}</p>
      {version.fingerprint !== version.sha && (
        <p className="m-0 text-xs text-text-secondary">Working tree modified since this build</p>
      )}
    </div>
  );
};
