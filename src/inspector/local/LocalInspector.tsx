/**
 * The inspector for a model on this machine: its name (⧉ copies the Model
 * ID), ⋯ and ×; one line of facts (the library row's); the fit sentence;
 * Load and Chat (Chat and Unload once loaded, with its state); the Model ID
 * to use it by; How it runs (How it ran once loaded); ▸ Log with the
 * server's last line once loaded; and Details, closed. A failed load says
 * why in one line, with Show log.
 *
 * The ⋯ menu's Load with options… and Load and pin, a launch-line token's
 * Change for this model…, and `?serve=1` (the native menu's Serve) open
 * the Load with options sheet in the inspector's place; the address is
 * read here, acted on once, and stripped. Verify files and Check for
 * updates (offered for a model from Hugging Face) start the check at once
 * and open Details on Source, where its line is.
 *
 * @module inspector/local/LocalInspector
 */

import { useCallback, useEffect, useRef, useState, type FC } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { MoreHorizontal, X } from 'lucide-react';
import { libraryIntent, paths } from '../../app/paths';
import { CopyValue } from '../../components/CopyValue';
import { InstallProgress } from '../../components/SetupWizard/InstallProgress';
import { RESIDENCY_MARK, StatusMark } from '../../components/status/StatusMark';
import { RESIDENCY_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { useToastContext } from '../../contexts/ToastContext';
import { useSettings } from '../../hooks/useSettings';
import type { ServerViewModel } from '../../hooks/useServers';
import { useEngineInstall } from '../../places/library/engineInstall';
import { modelFacts } from '../../places/library/groups';
import { Menu, type MenuItem } from '../../places/library/Menu';
import { useResidency } from '../../places/library/useResidency';
import { refreshModels } from '../../services/modelEvents';
import { openUrl } from '../../services/platform';
import { getTransport } from '../../services/transport';
import type { GgufModel } from '../../types';
import { formatError } from '../../utils/errors';
import { getHuggingFaceUrl } from '../../utils/format';
import { componentRoleLabel } from '../../utils/imageFamily';
import { LoadOptionsSheet, type SheetOpen } from '../LoadOptionsSheet';
import { LogLink } from './LogLink';
import type { Update } from './Change';
import { Details } from './Details';
import { HowItRuns } from './HowItRuns';
import { fitSentence } from './rows';
import { useExplain } from './useExplain';
import { useLaunchFacts } from './useLaunchFacts';
import { useLoad } from './useLoad';
import { useModelDetail } from './useModelDetail';
import { useRetagModel } from './useRetagModel';
import { useSdStatus } from './useSdStatus';
import { useVerification } from './useVerification';

export interface LocalInspectorProps {
  model: GgufModel;
  servers: ServerViewModel[];
  /** A server started here for a chat: open that chat on its port. */
  onServerStarted: (server: ServerViewModel) => void;
  onStopServer: (modelId: number) => Promise<void>;
  /** Remove the model from the library (its file stays); rejects with the daemon's refusal. */
  onRemoveModel: (id: number) => Promise<void>;
  onUpdateModel: Update;
  /** Open Settings where the image runtime is installed. */
  onOpenSystemSettings?: () => void;
}

/** The detail's reads and tag writes: one set for every render, so a read does not start another. */
const DETAIL_CALLS = {
  getModelDetail: (id: number) => getTransport().getModelDetail(id),
  onAddTag: (id: number, tag: string) => getTransport().addModelTag(id, tag),
  onRemoveTag: (id: number, tag: string) => getTransport().removeModelTag(id, tag),
  onRefresh: refreshModels,
};

export const LocalInspector: FC<LocalInspectorProps> = (props) => {
  const { model, servers, onServerStarted, onStopServer, onRemoveModel, onUpdateModel, onOpenSystemSettings } = props;
  const navigate = useNavigate();
  const location = useLocation();
  const { showToast } = useToastContext();
  const { confirm } = useConfirmContext();
  const { settings } = useSettings();
  const [profile, setProfile] = useState<string | null>(null);
  const [reveal, setReveal] = useState(0);
  const [sheet, setSheet] = useState<SheetOpen | null>(null);

  const draws = model.imageFamily !== undefined;
  const port = servers.find((s) => s.modelId === model.id)?.port;
  const loaded = port !== undefined;
  const explain = useExplain(model.id, profile);
  const detail = useModelDetail({ modelId: model.id, ...DETAIL_CALLS });
  const facts = useLaunchFacts(model.id, loaded);
  const state = useResidency().get(model.id) ?? facts?.state ?? 'loaded';
  const load = useLoad(model, draws, onServerStarted, onStopServer);
  const install = useEngineInstall();
  const sd = useSdStatus(draws);
  const sdMissing = draws && sd.status?.installed === false;
  // An image model's parts the library says are missing: Load waits for them, as the sheet's does.
  const partsMissing = draws ? model.missingComponents.map(componentRoleLabel) : [];
  // Tags decide flags (`agent` templates, `mtp` drafting), so a retag reads the launch again too.
  const { reload: reloadDetail } = detail;
  const { reload: reloadExplain } = explain;
  const afterRetag = useCallback(async () => {
    await reloadDetail();
    void reloadExplain();
  }, [reloadDetail, reloadExplain]);
  const retag = useRetagModel({ modelId: model.id, reload: afterRetag, showToast, confirm });
  const verification = useVerification(model.id, model.name);
  /** Start a check, then open Details on Source, where its line is. */
  const inSource = (start: () => void) => () => {
    start();
    setReveal((n) => n + 1);
  };

  const tagged = (tag: string) => detail.tags.some((t) => t.toLowerCase() === tag);

  // `?serve=1` (the native menu's Serve) opens the sheet once, then leaves the address.
  const served = useRef<string | null>(null);
  const serveAsked = libraryIntent(location.search).serve;
  useEffect(() => {
    if (!serveAsked || served.current === location.key) return;
    served.current = location.key;
    setSheet({ pin: false, origin: 'menu' });
    void navigate(location.pathname, { replace: true });
  }, [serveAsked, location.key, location.pathname, navigate]);

  const changed = () => {
    void explain.reload();
    void detail.reload();
  };
  const copied = (what: string) => () => showToast(`Copied ${what}`, 'success');
  const repo = getHuggingFaceUrl(model.hfRepoId);

  const remove = async () => {
    const yes = await confirm({
      title: `Remove ${model.name} from the library?`,
      description: 'The file stays on disk.',
      confirmLabel: 'Remove',
      variant: 'danger',
    });
    if (!yes) return;
    await onRemoveModel(model.id).catch((err: unknown) => showToast(`Could not remove ${model.name}: ${formatError(err)}`, 'error'));
  };

  const items: MenuItem[] = [
    { label: 'Load with options…', onPick: () => setSheet({ pin: false, origin: 'menu' }) },
    { label: 'Load and pin', onPick: () => setSheet({ pin: true, origin: 'menu' }) },
    { label: 'Verify files', onPick: inSource(verification.verify), gapBefore: true },
    ...(model.hfRepoId ? [{ label: 'Check for updates', onPick: inSource(verification.checkUpdates) }] : []),
    {
      label: 'Re-detect tags',
      onPick: () => void retag.handleRetag(false),
      gapBefore: true,
      title: 'Adds the capability tags the GGUF implies and removes none.',
    },
    {
      label: 'Rebuild tags…',
      onPick: () => void retag.handleRetag(true),
      title: 'Derives the detected tags again from the GGUF, dropping those it no longer finds, hand-added ones included.',
    },
    { label: 'Copy path', onPick: () => void navigator.clipboard.writeText(model.filePath).then(copied('the path')), gapBefore: true },
    ...(repo ? [{ label: 'Open on Hugging Face', onPick: () => void openUrl(repo) }] : []),
    { label: 'Benchmark…', onPick: () => navigate(paths.benchmarks([model.id])), gapBefore: true },
    { label: 'Remove from library…', onPick: () => void remove(), gapBefore: true, danger: true },
  ];

  const fit = loaded ? null : fitSentence(explain.explanation?.plan.fit ?? null);
  const installing = load.needsLlama && install.installing;

  if (sheet)
    return (
      <LoadOptionsSheet
        {...sheet}
        model={model}
        detail={detail.modelDetail?.id === model.id ? detail.modelDetail : null}
        explanation={explain.explanation}
        failed={explain.failed}
        speculative={tagged('mtp') || model.serverDefaults?.mtp != null || !!explain.explanation?.plan.args.some((a) => a.decision === 'mtp')}
        basePort={settings?.llamaBasePort ?? null}
        load={load}
        onUpdateModel={onUpdateModel}
        onSaved={changed}
        onClose={() => setSheet(null)}
      />
    );

  return (
    <div className="flex flex-col gap-lg p-md">
      {/* The name and the main actions stay in view while the rest scrolls (IN-1). */}
      <div className="sticky top-0 z-10 -mx-md -mt-md flex flex-col gap-sm bg-surface px-md pb-sm pt-md">
        <div className="flex flex-col gap-xs">
          <div className="flex items-center gap-xs">
            <h2 className="m-0 min-w-0 truncate text-base font-semibold text-text">{model.name}</h2>
            {model.openaiId !== null && (
              <CopyValue value={model.openaiId} label={`Copy the Model ID ${model.openaiId}`} onCopied={copied(model.openaiId)} />
            )}
            <span className="flex-1" />
            <Menu iconOnly name="More" label={<Icon icon={MoreHorizontal} size={14} />} items={items} />
            <Button variant="ghost" size="sm" iconOnly aria-label="Close" title="Close" onClick={() => navigate(paths.library())}>
              <Icon icon={X} size={14} />
            </Button>
          </div>
          <p className="m-0 text-xs text-text-secondary tabular-nums">{modelFacts(model).filter(Boolean).join(' · ')}</p>
          {loaded && <StatusMark kind={RESIDENCY_MARK[state]} word={RESIDENCY_WORDS[state]} className="text-xs" />}
          {fit && <p className="m-0 text-sm text-text">{fit}</p>}
        </div>

        <div className="flex flex-col gap-sm">
          <div className="flex items-center gap-sm">
            {loaded ? (
              <>
                <Button onClick={() => navigate(paths.chat(model.id))}>Chat</Button>
                <Button variant="secondary" onClick={load.unload} isLoading={load.busy === 'unload'}>
                  Unload
                </Button>
              </>
            ) : (
              <>
                <Button onClick={load.load} isLoading={load.busy === 'load' || installing} disabled={sdMissing || partsMissing.length > 0 || load.busy !== null}>
                  {load.needsLlama ? 'Install llama.cpp' : 'Load'}
                </Button>
                {!draws && (
                  <Button variant="secondary" onClick={load.loadForChat} isLoading={load.busy === 'chat'} disabled={load.busy !== null || installing}>
                    Chat
                  </Button>
                )}
              </>
            )}
          </div>
          {installing && <InstallProgress progress={install.progress} />}
          {partsMissing.length > 0 && <p className="m-0 text-xs text-text-secondary">{`▲ ${partsMissing.join(', ')} missing.`}</p>}
          {sdMissing && (
            <p className="m-0 flex items-center gap-sm text-xs text-text-secondary">
              Drawing needs stable-diffusion.cpp.
              {onOpenSystemSettings && (
                <Button variant="link" size="sm" className="text-xs font-normal" onClick={onOpenSystemSettings}>
                  Install
                </Button>
              )}
            </p>
          )}
          {load.error && (
            <p role="alert" className="m-0 flex items-baseline gap-xs text-xs text-text">
              <StatusMark kind="failed" label="Failed" />
              {`${load.error.action === 'unload' ? "Didn't unload" : "Didn't load"}: ${load.error.message}`}
              {load.error.action === 'load' && (
                <Button variant="link" size="sm" className="h-auto p-0 text-xs font-normal" onClick={() => navigate(paths.log(model.id))}>
                  Show log
                </Button>
              )}
            </p>
          )}
          {model.openaiId !== null && (
            <p className="m-0 flex items-center gap-xs text-xs text-text-secondary">
              Model ID <code className="font-mono text-text">{model.openaiId}</code>
              <CopyValue value={model.openaiId} label="Copy the Model ID" onCopied={copied(model.openaiId)} />
            </p>
          )}
        </div>
      </div>

      <HowItRuns
        model={model}
        detail={detail.modelDetail?.id === model.id ? detail.modelDetail : null}
        explanation={explain.explanation}
        failed={explain.failed}
        narration={loaded ? (facts?.narration ?? null) : undefined}
        profiles={settings?.inferenceProfiles ?? []}
        profile={profile}
        onProfile={setProfile}
        onUpdateModel={onUpdateModel}
        onChanged={changed}
        onChangeToken={(focus) => setSheet({ pin: false, focus, origin: 'line' })}
      />

      {port !== undefined && <LogLink modelId={model.id} port={port} />}

      <Details
        model={model}
        detail={detail}
        loaded={loaded}
        onUpdateModel={onUpdateModel}
        onChanged={() => void explain.reload()}
        verification={verification}
        reveal={reveal}
      />
    </div>
  );
};
