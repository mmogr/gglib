/**
 * Setup's step 2, Engines: llama.cpp, which every chat model runs on,
 * installing on arrival with its progress, then ready; stable-diffusion.cpp,
 * for drawing, ready or skipped, since chat works without it; and faster
 * downloads, which First model's Get it sets up where Python 3 is there
 * (`useFastDownloads`).
 *
 * An installed engine says so in the word every surface uses
 * (`ENGINE_WORDS`), llama.cpp's with why it does not work when it does
 * not; one that needs building says Not installed. llama.cpp's failure is
 * an error under its line, with Retry;
 * stable-diffusion.cpp's and the accelerator's are warnings (SU-2). Where
 * only a terminal can build llama.cpp, the step names the command the
 * daemon gives, with Check again, and looks for the build by itself every
 * few seconds (`useEngines`). Continue needs llama.cpp ready; Skip goes on
 * without it, and the Library's banner and Settings › Engines take it up.
 *
 * @module places/setup/EnginesStep
 */

import type { FC } from 'react';
import { CopyValue } from '../../components/CopyValue';
import { InstallProgress } from '../../components/InstallProgress';
import { StatusMark } from '../../components/status/StatusMark';
import { ENGINE_WORDS, FAST_DOWNLOADS_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { ENGINE_PRODUCT } from '../../types/setup';
import type { EngineStatus } from '../../types/setup';
import { Item, Problem, Step } from './Step';
import { installable, needsBuild, type EngineView, type SetupEngines } from './useEngines';
import type { FastDownloads } from './useFastDownloads';

const SKIPPED = 'skipped — chat works without it';

/** The command a terminal builds `status`'s engine with, with ⧉. */
const Command: FC<{ status: EngineStatus }> = ({ status }) => {
  const command = status.installCommand;
  return (
    <span className="inline-flex items-center gap-xs">
      <code className="font-mono text-text">{command}</code>
      <CopyValue value={command} label="Copy the command" />
    </span>
  );
};

/** An installed engine's state, in the word every surface uses: working, or not. */
const Installed: FC<{ status: EngineStatus }> = ({ status }) => (
  <StatusMark kind={status.state === 'not_working' ? 'failed' : 'on'} word={ENGINE_WORDS[status.state]} />
);

/** The progress under an engine's line while it installs. */
const Progress: FC<{ view: EngineView; product: string }> = ({ view, product }) =>
  view.installing === null ? null : (
    <InstallProgress progress={view.installing === 'starting' ? null : view.installing} product={product} />
  );

const Llama: FC<{ view: EngineView; readError: string | null; onRetry: () => void; onRecheck: () => void }> = ({
  view,
  readError,
  onRetry,
  onRecheck,
}) => {
  const { status } = view;
  let state;
  if (view.installing !== null) state = <StatusMark kind="changing" word="installing" />;
  else if (status?.installed) state = <Installed status={status} />;
  else if (view.error !== null) state = <StatusMark kind="failed" word="did not install" />;
  else if (needsBuild(status)) state = <StatusMark kind="attention" word={ENGINE_WORDS.not_installed} />;
  else if (status === null && readError === null) state = <StatusMark kind="changing" word="reading" />;
  return (
    <Item name={ENGINE_PRODUCT.llama} state={state}>
      <Progress view={view} product={ENGINE_PRODUCT.llama} />
      {view.installing === null && view.error !== null && <Problem onRetry={onRetry}>{view.error}</Problem>}
      {readError !== null && <Problem onRetry={onRecheck}>{readError}</Problem>}
      {view.installing === null && status?.broken && <Problem>{status.broken}</Problem>}
      {status && view.installing === null && needsBuild(status) && (
        <>
          <p className="m-0 flex flex-wrap items-center gap-xs">
            This Mac needs llama.cpp built from source. In a terminal: <Command status={status} />
          </p>
          <p className="m-0 flex items-center gap-sm">
            <Button variant="secondary" size="sm" onClick={onRecheck}>
              Check again
            </Button>
            <span>gglib looks again every few seconds.</span>
          </p>
        </>
      )}
      {status?.warning && installable(status) && <p className="m-0">{status.warning}</p>}
    </Item>
  );
};

const Drawing: FC<{ view: EngineView; onRetry: () => void }> = ({ view, onRetry }) => {
  const { status } = view;
  const skipped = view.error !== null || needsBuild(status);
  let state;
  if (view.installing !== null) state = <StatusMark kind="changing" word="installing" />;
  else if (status?.installed) state = <Installed status={status} />;
  else if (skipped) state = <StatusMark kind="attention" word={SKIPPED} />;
  else if (installable(status)) state = <StatusMark kind="off" word="waiting" />;
  return (
    <Item name={`${ENGINE_PRODUCT.stable_diffusion} (for drawing)`} state={state}>
      <Progress view={view} product={ENGINE_PRODUCT.stable_diffusion} />
      {view.installing === null && view.error !== null && (
        <Problem warning onRetry={onRetry}>
          {view.error}
        </Problem>
      )}
      {status && view.installing === null && needsBuild(status) && (
        <p className="m-0 flex flex-wrap items-center gap-xs">
          In a terminal: <Command status={status} />
        </p>
      )}
      {status?.warning && installable(status) && <p className="m-0">{status.warning}</p>}
    </Item>
  );
};

const Fast: FC<{ fast: FastDownloads['fast']; onRetry: () => void }> = ({ fast, onRetry }) => {
  if (fast === 'later') return <Item name="Faster downloads" state={<StatusMark kind="off" word="set up when you get a model" />} />;
  if (fast === 'ready') return <Item name="Faster downloads" state={<StatusMark kind="on" word={FAST_DOWNLOADS_WORDS.on} />} />;
  if (fast === 'no-python') {
    return <Item name="Faster downloads" state={<StatusMark kind="off" word="needs Python 3; downloads still work" />} />;
  }
  if (fast === 'setting-up') return <Item name="Faster downloads" state={<StatusMark kind="changing" word="setting up" />} />;
  return (
    <Item name="Faster downloads" state={<StatusMark kind="attention" word="not set up; downloads still work" />}>
      <Problem warning onRetry={onRetry}>
        {fast.failed}
      </Problem>
    </Item>
  );
};

export const EnginesStep: FC<{ engines: SetupEngines; fast: FastDownloads; onNext: () => void; onBack: () => void }> = ({
  engines,
  fast,
  onNext,
  onBack,
}) => {
  const ready = engines.llama.status?.installed === true && engines.llama.installing === null;
  return (
    <Step
      n={2}
      title="Engines"
      onBack={onBack}
      actions={
        <>
          {!ready && (
            <Button variant="ghost" size="sm" onClick={onNext}>
              Skip
            </Button>
          )}
          <Button onClick={onNext} disabled={!ready}>
            Continue
          </Button>
        </>
      }
    >
      <Llama
        view={engines.llama}
        readError={engines.readError}
        onRetry={() => engines.retry('llama')}
        onRecheck={engines.recheck}
      />
      <Drawing view={engines.sd} onRetry={() => engines.retry('stable_diffusion')} />
      <Fast fast={fast.fast} onRetry={fast.retry} />
    </Step>
  );
};
