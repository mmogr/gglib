/**
 * One engine's row in Settings › Engines, the same for llama.cpp and for
 * stable-diffusion.cpp: the GUI face of
 * `gglib config llama|sd install|status|uninstall`.
 *
 * The row says what is installed in one line (the release, the download or
 * the acceleration, prebuilt or built from source, and its state), then
 * offers what can be done: Install where nothing is, Update to the pin
 * (asked first) where an older one is, and under ⋯ Reinstall and Remove…
 * (asked first). Nothing is offered as a button where the daemon would
 * refuse it: a source build's install or update is the terminal's, named by
 * its command. While a model runs on the engine every action is disabled,
 * saying which model to stop (ST-11). ▸ Details holds the install record,
 * the binary and where it stands against its pin. All state is `useEngine`'s;
 * every fact is the daemon's engine status.
 *
 * @module places/settings/EngineRow
 */

import type { FC, ReactNode } from 'react';
import { MoreHorizontal } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { StatusMark, type MarkKind } from '../../components/status/StatusMark';
import { ENGINE_WORDS } from '../../components/status/words';
import { InstallProgress } from '../../components/InstallProgress';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { useEngine } from '../../hooks/useEngine';
import { ENGINE_PRODUCT } from '../../types/setup';
import type { EngineStatus, RuntimeKind } from '../../types/setup';
import type { EngineState } from '../../types/generated/EngineState';
import { Menu, type MenuItem } from '../../components/ui/Menu';
import { Disclosure } from './Disclosure';
import { EngineDetails } from './EngineDetails';
import { pinStanding } from './pinStanding';

/** What each engine serves and what its removal takes, for the Remove… question. */
const WORDS: Record<RuntimeKind, { serves: string; removes: string }> = {
  llama: { serves: 'chat models', removes: 'its binaries, its install record and any source checkout' },
  stable_diffusion: { serves: 'image models', removes: 'sd-server, its library and its install record' },
};

/** The shape each engine state is drawn as; its word is `ENGINE_WORDS`'. */
const MARK: Record<EngineState, MarkKind> = {
  installed: 'on',
  not_working: 'failed',
  not_installed: 'off',
};

/** The row's one line: what is installed and how, or what an install would take. */
function lineOf(status: EngineStatus): string {
  if (!status.installed) {
    return [status.pin.targetRelease && `Pinned to ${status.pin.targetRelease}`, status.download]
      .filter(Boolean)
      .join(' · ');
  }
  const install = status.install;
  const release = install?.release ?? status.pin.installedRelease;
  const how = install ? (install.built ? 'built from source' : 'prebuilt') : null;
  return [release, install?.platform, how].filter(Boolean).join(' · ');
}

/** A sentence under the row, in its weight. */
const Line: FC<{ tone?: 'danger' | 'muted'; children: ReactNode }> = ({ tone = 'muted', children }) => (
  <p className={`m-0 text-xs ${tone === 'danger' ? 'text-danger' : 'text-text-secondary'}`}>{children}</p>
);

export const EngineRow: FC<{ kind: RuntimeKind }> = ({ kind }) => {
  const engine = useEngine(kind);
  const { status, installing, uninstalling } = engine;
  const { confirm } = useConfirmContext();
  const product = ENGINE_PRODUCT[kind];
  const heading = `engine-${kind}-heading`;

  const standing = status ? pinStanding(status.pin, status.installCommand) : null;
  const serving = status?.serving ?? null;
  const stopFirst = serving ? `Stop ${serving} first.` : undefined;
  const busy = installing || uninstalling || serving !== null;
  // Whether an install is a download is the daemon's to say (`updateDownloads`);
  // where one fits and still the install is not one, a source build was here.
  const missing = !!status && !status.installed;
  const offersInstall = missing && !!status?.pin.updateDownloads;
  const builtBefore = missing && status?.download !== null && !status?.pin.updateDownloads;
  const offersUpdate = !!status?.installed && !!standing?.update;
  // Over a download the install is a download again (`plan_install`); over a
  // source build it would build, which is the terminal's.
  const offersReinstall =
    !!status?.installed && status.download !== null && status.install?.built === false && !status.pin.rebuilds;

  const update = async () => {
    const yes = await confirm({
      title: `Update ${product} to ${standing?.update ?? 'the pinned release'}?`,
      description: 'Downloads the pinned release for this machine and replaces the one installed.',
      confirmLabel: 'Update',
    });
    if (yes) engine.runInstall();
  };

  const remove = async () => {
    const { serves, removes } = WORDS[kind];
    const yes = await confirm({
      title: `Remove ${product}?`,
      description:
        `Removes ${product}: ${removes}. Your ${serves} stay on disk, but none can be ` +
        'served until it is installed again.',
      confirmLabel: 'Remove',
      variant: 'danger',
    });
    if (yes) await engine.runUninstall();
  };

  const more: MenuItem[] = [
    ...(offersReinstall
      ? [{ label: 'Reinstall', disabled: busy, title: stopFirst, onPick: engine.runInstall }]
      : []),
    { label: 'Remove…', danger: true, disabled: busy, title: stopFirst, onPick: () => void remove() },
  ];

  return (
    <section aria-labelledby={heading} className="flex flex-col gap-sm">
      <div className="flex items-start justify-between gap-md">
        <div className="flex min-w-0 flex-col gap-xs">
          <h3 id={heading} className="m-0 text-sm font-semibold text-text">
            {product}
          </h3>
          {status && (
            <div className="flex flex-wrap items-center gap-x-md gap-y-xs">
              <span className="text-xs text-text-secondary tabular-nums">{lineOf(status)}</span>
              <StatusMark kind={MARK[status.state]} word={ENGINE_WORDS[status.state]} className="text-xs" />
            </div>
          )}
          {engine.loadingStatus && !status && <Line>Reading {product}…</Line>}
        </div>
        <div className="flex shrink-0 items-center gap-xs">
          {offersInstall && (
            <Button variant="secondary" size="sm" disabled={busy} title={stopFirst} onClick={engine.runInstall}>
              Install
            </Button>
          )}
          {offersUpdate && (
            <Button variant="secondary" size="sm" disabled={busy} title={stopFirst} onClick={() => void update()}>
              Update to {standing?.update}…
            </Button>
          )}
          {status?.installed && (
            <Menu iconOnly name={`More for ${product}`} label={<Icon icon={MoreHorizontal} size={14} />} items={more} />
          )}
        </div>
      </div>

      {serving && <Line>{stopFirst}</Line>}
      {engine.statusError && <Line tone="danger">{engine.statusError}</Line>}
      {status?.broken && <Line tone="danger">{status.broken}</Line>}
      {standing?.fromTerminal && <Line>{standing.fromTerminal}</Line>}
      {(offersInstall || offersUpdate || offersReinstall) && status?.warning && (
        <StatusMark kind="attention" word={status.warning} className="text-xs" />
      )}
      {builtBefore && (
        <Line>
          {product} was built from source here before, so it is built again from a terminal:{' '}
          <code className="font-mono">{status?.installCommand}</code>.
        </Line>
      )}
      {missing && status?.downloadUnavailable && (
        <Line>
          {status.downloadUnavailable.replace(/\.$/, '')}. Build it from source with{' '}
          <code className="font-mono">{status.installCommand}</code>.
        </Line>
      )}

      {installing && <InstallProgress progress={engine.progress} product={product} />}
      {engine.installError && <Line tone="danger">{engine.installError}</Line>}
      {engine.installedVersion && <Line>{product} {engine.installedVersion} is installed.</Line>}
      {engine.uninstallError && <Line tone="danger">{engine.uninstallError}</Line>}
      {engine.uninstallResult && <Line>{engine.uninstallResult}</Line>}

      {status?.installed && (
        <Disclosure label="Details">
          <EngineDetails status={status} standing={standing?.text ?? null} />
        </Disclosure>
      )}
    </section>
  );
};
