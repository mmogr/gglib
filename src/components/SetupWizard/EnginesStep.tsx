/**
 * The setup wizard's engines step: llama.cpp, which runs chat models, and
 * stable-diffusion.cpp, which draws images, each installed from its pinned
 * download by one button.
 *
 * llama.cpp goes first, because no chat model runs without it.
 * stable-diffusion.cpp goes second, and its failure is only a warning: chat
 * works without it, and Settings installs it later. An engine with no
 * download for this machine names the command that builds it. The installs
 * are the daemon's, the stream the Settings engine card reads.
 */

import { FC, useCallback, useEffect, useRef, useState } from 'react';
import { ArrowRight, Download, SkipForward } from 'lucide-react';
import { Button } from '../ui/Button';
import { Banner } from '../ui/Banner';
import { Icon } from '../ui/Icon';
import { InstallProgress } from './InstallProgress';
import { getEngineStatus, streamEngineInstall } from '../../services/transport/api/setup';
import { ENGINE_PRODUCT } from '../../types/setup';
import type { EngineStatus, LlamaProgressEvent, RuntimeKind } from '../../types/setup';
import { formatError } from '../../utils/errors';

/** The engines, in install order, and what each is for. */
const ENGINES: readonly { kind: RuntimeKind; does: string; required: boolean }[] = [
  { kind: 'llama', does: 'runs chat models', required: true },
  { kind: 'stable_diffusion', does: 'draws images', required: false },
];

/** Each engine's status, as last read. */
export type EngineStatuses = Partial<Record<RuntimeKind, EngineStatus>>;

/**
 * Whether the step's button installs `status`'s engine: it is missing and
 * the daemon says its install is a download, not a source build the command
 * line runs.
 */
const wanted = (status: EngineStatus | undefined): boolean =>
  !!status && !status.installed && status.pin.updateDownloads;

/** One line on where `status`'s engine stands. */
function standing(status: EngineStatus): string {
  if (status.installed) {
    return status.install ? `Installed: ${status.install.release}` : 'Installed';
  }
  if (status.download !== null && !status.pin.updateDownloads) {
    return `Built from source here before. Build it again from a terminal: ${status.updateCommand}.`;
  }
  if (status.download === null) {
    return `${(status.downloadUnavailable ?? 'No download fits this machine').replace(/\.$/, '')}. Build it from a terminal: ${status.installCommand}.`;
  }
  return `Not installed. Installs the ${status.download} download.`;
}

export const EnginesStep: FC<{
  /** Moves on, with each engine's status as last read, for the summary. */
  onNext: (engines: EngineStatuses) => void;
  onBack: () => void;
}> = ({ onNext, onBack }) => {
  const [statuses, setStatuses] = useState<EngineStatuses>({});
  const [readError, setReadError] = useState<string | null>(null);
  // No way on until the engines have been read: a Continue pressed before
  // then would skip an install nobody was shown.
  const [loaded, setLoaded] = useState(false);
  const [installing, setInstalling] = useState<RuntimeKind | null>(null);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<LlamaProgressEvent | null>(null);
  const [errors, setErrors] = useState<Partial<Record<RuntimeKind, string>>>({});
  const stopReading = useRef<(() => void) | null>(null);

  const read = useCallback(async () => {
    try {
      const found = await Promise.all(ENGINES.map(({ kind }) => getEngineStatus(kind)));
      setStatuses(Object.fromEntries(found.map((status) => [status.kind, status])));
      setReadError(null);
    } catch (err) {
      setReadError(formatError(err));
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void read();
  }, [read]);

  // Unmounting stops the reading, not the install: the daemon finishes it.
  useEffect(() => () => stopReading.current?.(), []);

  /** Install `kind`, settling with null when it finished, or with why it did not. */
  const installOne = (kind: RuntimeKind) =>
    new Promise<string | null>((resolve) => {
      let settled = false;
      const settle = (error: string | null) => {
        if (settled) return;
        settled = true;
        resolve(error);
      };
      setInstalling(kind);
      setProgress(null);
      stopReading.current = streamEngineInstall(
        kind,
        (event) => {
          setProgress(event);
          if (event.type === 'completed') settle(null);
          else if (event.type === 'failed') settle(event.message);
        },
        settle,
        () => settle('The connection to the install ended before it reported a result.'),
      );
    });

  const toInstall = ENGINES.filter(({ kind }) => wanted(statuses[kind])).map(({ kind }) => kind);

  const handleInstall = async () => {
    setRunning(true);
    setErrors({});
    for (const kind of toInstall) {
      const error = await installOne(kind);
      if (error) setErrors((prev) => ({ ...prev, [kind]: error }));
    }
    setInstalling(null);
    await read();
    setRunning(false);
  };

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-xl font-semibold text-text mb-2">Engines</h2>
        <p className="text-text-secondary leading-relaxed">
          gglib runs models with two engines, each installed from the release it is pinned to.
        </p>
      </div>

      {!loaded && <p className="m-0 text-sm text-text-muted">Reading the engines…</p>}
      {readError && (
        <Banner variant="danger" title="Could not read the engines">
          <span className="break-all">{readError}</span>
        </Banner>
      )}

      <div className="flex flex-col gap-2">
        {ENGINES.map(({ kind, does }) => {
          const status = statuses[kind];
          return (
            <div key={kind} className="py-2 px-3 bg-background-secondary rounded-lg border border-border">
              <p className="m-0 text-sm text-text">
                <span className="font-medium">{ENGINE_PRODUCT[kind]}</span>
                <span className="text-text-muted">, which {does}</span>
              </p>
              {status && <p className="m-0 text-xs text-text-muted">{standing(status)}</p>}
              {installing === kind && (
                <div className="mt-2">
                  <InstallProgress progress={progress} product={ENGINE_PRODUCT[kind]} />
                </div>
              )}
            </div>
          );
        })}
      </div>

      {!running &&
        ENGINES.map(({ kind }) => {
          const warning = statuses[kind]?.warning;
          return wanted(statuses[kind]) && warning ? (
            <Banner key={kind} variant="warning">
              {warning}
            </Banner>
          ) : null;
        })}

      {ENGINES.map(({ kind, required }) =>
        errors[kind] ? (
          <Banner
            key={kind}
            variant={required ? 'danger' : 'warning'}
            title={`${ENGINE_PRODUCT[kind]} did not install`}
          >
            <span className="break-all">{errors[kind]}</span>
            {!required && ' Chat works without it; install it later from Settings → System.'}
          </Banner>
        ) : null,
      )}

      <div className="flex items-center justify-between pt-2">
        <div>
          {!running && (
            <Button variant="ghost" onClick={onBack} size="sm">
              Back
            </Button>
          )}
        </div>
        {loaded && !running && (
          <div className="flex items-center gap-2">
            {toInstall.length > 0 ? (
              <>
                <Button
                  variant="ghost"
                  onClick={() => onNext(statuses)}
                  size="sm"
                  leftIcon={<Icon icon={SkipForward} size={14} />}
                >
                  Skip
                </Button>
                <Button
                  variant="primary"
                  onClick={() => void handleInstall()}
                  leftIcon={<Icon icon={Download} size={16} />}
                >
                  {Object.keys(errors).length > 0 ? 'Retry' : 'Install'}
                </Button>
              </>
            ) : (
              <Button
                variant="primary"
                onClick={() => onNext(statuses)}
                rightIcon={<Icon icon={ArrowRight} size={16} />}
              >
                Continue
              </Button>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
