/**
 * The endpoint place (`/endpoint`): the address, what is loaded and how,
 * what it serves, how a client connects, who is using it, what gglib did
 * to recent requests, and the rest of the dashboard as sentences; the
 * inspector beside it while `?inspect=model:<id>` names a model.
 *
 * Facts are the daemon's: the endpoint's from `proxyRegistry` (its status,
 * re-read after each event), the rest from the one dashboard stream
 * (`useEndpointDashboard`), the devices from `remoteRegistry`. The
 * sections that need the dashboard are drawn once it has spoken, so
 * nothing says "nothing loaded" before it knows. ⋯ holds Live numbers (the
 * dashboard dialog, until Diagnostics › Live), Clear prompt cache, and
 * Endpoint settings… (General, where the endpoint's fields are today).
 *
 * Connect a client's Model ID starts at the inspected model, else the
 * pinned one, else the last loaded, else the default model (the setting
 * `gglib q` uses), else the library's smallest chat model, so Test it
 * never asks the endpoint to load the biggest one; Test it asks for that
 * model, or for the pin when the endpoint is pinned. Live numbers and
 * Clear prompt cache are offered only while they can act: the endpoint
 * up, and for Clear its key revealed.
 *
 * @module places/endpoint/EndpointPlace
 */

import { useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { MoreHorizontal } from 'lucide-react';
import { HANDED, setChatHandoff } from '../../app/chatHandoff';
import { inspectorClosed, inspectorSubject, paths, settingsAt } from '../../app/paths';
import { ConnectClient } from '../../components/ConnectClient';
import { ProxyDashboardModal } from '../../components/ProxyDashboardModal';
import { Icon } from '../../components/ui/Icon';
import { useToastContext } from '../../contexts/ToastContext';
import { useSettings } from '../../hooks/useSettings';
import { usePairedModels } from '../../hooks/usePairedModels';
import { useServers, type ServerViewModel } from '../../hooks/useServers';
import { Inspector } from '../../inspector/Inspector';
import { clearProxyCache } from '../../services/clients/proxyDashboard';
import { removeModel as removeLibraryModel, updateModel } from '../../services/modelEvents';
import { useModelLibrary } from '../../services/modelRegistry';
import { useEndpointFacts } from '../../services/proxyRegistry';
import { useRemoteState } from '../../services/remoteRegistry';
import { formatError } from '../../utils/errors';
import { Menu } from '../library/Menu';
import { useEndpointDashboard } from '../library/useResidency';
import { Activity } from './Activity';
import { Header } from './Header';
import { Loaded } from './Loaded';
import { Recent } from './Recent';
import { Serves } from './Serves';
import { useEndpointActions } from './useEndpointActions';
import { UsingIt } from './UsingIt';

export default function EndpointPlace() {
  const location = useLocation();
  const navigate = useNavigate();
  const { showToast } = useToastContext();
  const facts = useEndpointFacts();
  const dash = useEndpointDashboard();
  const actions = useEndpointActions();
  const { models } = useModelLibrary();
  const { servers, stopServer } = useServers();
  const paired = usePairedModels();
  const { status: remote } = useRemoteState();
  const { settings } = useSettings();
  const [picked, setPicked] = useState<number | null>(null);
  const [live, setLive] = useState(false);
  const [clearing, setClearing] = useState(false);
  const connectRef = useRef<HTMLDivElement>(null);

  const subject = inspectorSubject(location);
  const inspected = subject?.kind === 'model' ? subject.id : null;
  const snapshot = dash.snapshot;
  const slots = snapshot?.admission.slots ?? [];
  const loaded = slots.map((s) => ({ modelId: s.model_id, name: s.model_name }));
  const chatModels = models.filter((m) => m.imageFamily === undefined);
  const pinnedId = chatModels.find((m) => m.name === facts.pinnedModel)?.id ?? null;
  const lastLoaded = [...slots].sort((a, b) => a.resident_for_secs - b.resident_for_secs)[0]?.model_id ?? null;
  const settled = chatModels.find((m) => m.id === settings?.defaultModelId)?.id ?? null;
  const smallest = [...chatModels].sort((a, b) => a.sizeBytes - b.sizeBytes)[0]?.id ?? null;
  const modelId = picked ?? inspected ?? pinnedId ?? lastLoaded ?? settled ?? smallest;
  const up = facts.state === 'on' || facts.state === 'pinned';
  const canLive = up && dash.port !== null;
  // A dialog asked for while it could not open does not open later by itself.
  if (live && !canLive) setLive(false);

  const unload = (id: number) => void stopServer(id).catch((err) => showToast(formatError(err), 'error'));
  // Clear presents the proxy's key, so it waits for the reveal.
  const canClear = up && dash.port !== null && dash.apiKey !== undefined;
  const clear = async () => {
    if (!canClear || dash.port === null) return;
    setClearing(true);
    try {
      const result = await clearProxyCache(dash.host, dash.port, dash.apiKey ?? null);
      showToast(result.message || 'Prompt cache cleared', 'success');
    } catch (err) {
      showToast(formatError(err), 'error');
    } finally {
      setClearing(false);
    }
  };

  const handleServerStarted = (server: ServerViewModel) => {
    setChatHandoff({ modelId: server.modelId, port: server.port, modelName: server.modelName });
    navigate(paths.chat(server.modelId), { state: HANDED });
  };
  const removeModel = async (id: number) => {
    await removeLibraryModel(id);
    if (inspected === id) navigate(inspectorClosed(location));
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <section aria-label="Endpoint" className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <div className="flex shrink-0 items-center gap-sm px-md pb-sm pt-md">
          <h2 className="m-0 flex-1 text-base font-semibold text-text">Endpoint</h2>
          <Menu
            iconOnly
            name="More"
            label={<Icon icon={MoreHorizontal} size={14} />}
            items={[
              { label: 'Live numbers', onPick: () => setLive(true), disabled: !canLive },
              { label: 'Clear prompt cache', onPick: () => void clear(), disabled: !canClear },
              { label: 'Endpoint settings…', onPick: () => navigate(settingsAt(location, 'general'), { replace: true }), gapBefore: true },
            ]}
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto pb-md">
          <Header facts={facts} actions={actions} onTest={() => void actions.test(facts.pinnedModel !== null ? null : modelId)} />
          {up && snapshot && (
            <Loaded snapshot={snapshot} inspected={inspected} paired={paired} onUnload={unload} onPin={(id) => void actions.serve(id)} pinning={actions.busy !== null} />
          )}
          {up && (
            <Serves loaded={loaded} library={chatModels} pinnedModel={facts.pinnedModel} disabled={actions.busy !== null} onServe={(id) => void actions.serve(id)} />
          )}
          {up && facts.baseUrl !== null && (
            <div ref={connectRef}>
              <ConnectClient baseUrl={facts.baseUrl} keyState={facts.key} models={chatModels} modelId={modelId} onModel={setPicked} />
            </div>
          )}
          {up && snapshot && (
            <UsingIt snapshot={snapshot} devices={remote?.devices ?? []} onConnect={() => connectRef.current?.scrollIntoView?.({ behavior: 'smooth' })} />
          )}
          {up && snapshot && <Recent snapshot={snapshot} onAll={() => setLive(true)} />}
          {up && snapshot && (
            <Activity snapshot={snapshot} clearing={clearing} canClear={canClear} onClear={() => void clear()} onLive={() => setLive(true)} />
          )}
        </div>
      </section>

      <Inspector
        paired={paired}
        servers={servers}
        onServerStarted={handleServerStarted}
        onStopServer={stopServer}
        onRemoveModel={removeModel}
        onUpdateModel={updateModel}
        onOpenSystemSettings={() => navigate(settingsAt(location, 'system'), { replace: true })}
      />

      {dash.port !== null && <ProxyDashboardModal isOpen={live} onClose={() => setLive(false)} host={dash.host} port={dash.port} />}
    </div>
  );
}
