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
 * nothing says "nothing loaded" before it knows. Off, Loaded still lists
 * the running servers, which stay in memory. ⋯ holds Live numbers
 * (Diagnostics › Live), Clear prompt cache, and Endpoint settings….
 *
 * Connect a client's Model ID starts at the inspected model, else the
 * pinned one, else the last loaded, else the default model (the setting
 * `gglib q` uses), else the library's smallest chat model, so Test it
 * never asks the endpoint to load the biggest one; Test it asks for that
 * model, or for the pin when the endpoint is pinned. Clear prompt cache
 * is offered only while it can act: the endpoint up and its key revealed.
 *
 * @module places/endpoint/EndpointPlace
 */

import { useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { MoreHorizontal } from 'lucide-react';
import { openChatOn } from '../../app/openChat';
import { inspectorClosed, inspectorSubject, paths, withoutParam } from '../../app/paths';
import { ConnectClient } from '../../components/ConnectClient';
import { Icon } from '../../components/ui/Icon';
import { useToastContext } from '../../contexts/ToastContext';
import { useSettings } from '../../hooks/useSettings';
import { usePairedModels } from '../../hooks/usePairedModels';
import { useServers, type ServerViewModel } from '../../hooks/useServers';
import { Inspector } from '../../inspector/Inspector';
import { removeModel as removeLibraryModel, updateModel } from '../../services/modelEvents';
import { useModelLibrary } from '../../services/modelRegistry';
import { useEndpointFacts, useProxyState } from '../../services/proxyRegistry';
import { useRemoteState } from '../../services/remoteRegistry';
import { formatError } from '../../utils/errors';
import { Menu } from '../../components/ui/Menu';
import { useEndpointDashboard } from '../library/useResidency';
import { Activity } from './Activity';
import { Header } from './Header';
import { Loaded } from './Loaded';
import { residentRows } from './lines';
import { Recent } from './Recent';
import { Serves } from './Serves';
import { useClearCache } from './useClearCache';
import { useEndpointActions } from './useEndpointActions';
import { UsingIt } from './UsingIt';

export default function EndpointPlace() {
  const location = useLocation();
  const navigate = useNavigate();
  const { showToast } = useToastContext();
  const facts = useEndpointFacts();
  const { status: proxyStatus } = useProxyState();
  const dash = useEndpointDashboard();
  const actions = useEndpointActions();
  const { models } = useModelLibrary();
  const { servers, stopServer } = useServers();
  const paired = usePairedModels();
  const { status: remote } = useRemoteState();
  const { settings } = useSettings();
  const [picked, setPicked] = useState<number | null>(null);
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
  // Up, the dashboard's rows once it has spoken; off, the running servers,
  // drawn only when there are some.
  const resident = residentRows(snapshot, servers);
  const showLoaded = up ? snapshot !== null : resident.length > 0;
  // The menu's Connect a Client… (`?connect=1`): scrolled to once the block
  // is drawn, and the ask dropped from the address. With the endpoint off
  // there is no block to scroll to, and the ask is dropped at once, so it
  // cannot scroll the page later when the endpoint comes on. Before the
  // first status read there is no telling, so the ask waits for it.
  const connectAsked = new URLSearchParams(location.search).get('connect') === '1';
  useEffect(() => {
    if (!connectAsked || ((up || proxyStatus === null) && !connectRef.current)) return;
    connectRef.current?.scrollIntoView?.({ behavior: 'smooth' });
    navigate(withoutParam(location, 'connect'), { replace: true });
  });
  const toLive = () => navigate(paths.diagnostics('live'));

  const unload = (id: number) => void stopServer(id).catch((err) => showToast(formatError(err), 'error'));
  // Clear presents the proxy's key, so it waits for the reveal; and only while the endpoint is up.
  const cache = useClearCache(dash);
  const canClear = up && cache.canClear;
  const clear = () => (canClear ? cache.clear() : Promise.resolve());

  // A model started for a chat opens its chat, with the port it was served on.
  const handleServerStarted = (server: ServerViewModel) => {
    void openChatOn(navigate, { id: server.modelId, port: server.port, name: server.modelName });
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
              { label: 'Live numbers', onPick: toLive },
              { label: 'Clear prompt cache', onPick: () => void clear(), disabled: !canClear },
              { label: 'Endpoint settings…', onPick: () => navigate(paths.settings('endpoint')), gapBefore: true },
            ]}
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto pb-md">
          <Header facts={facts} actions={actions} onTest={() => void actions.test(facts.pinnedModel !== null ? null : modelId)} />
          {showLoaded && (
            <Loaded snapshot={snapshot} rows={resident} inspected={inspected} paired={paired} onUnload={unload} onPin={(id) => void actions.serve(id)} pinning={actions.busy !== null} />
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
          {up && snapshot && <Recent snapshot={snapshot} onAll={toLive} />}
          {up && snapshot && (
            <Activity snapshot={snapshot} clearing={cache.clearing} canClear={canClear} onClear={() => void clear()} onLive={toLive} />
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
        onOpenSystemSettings={() => navigate(paths.settings('engines'))}
      />
    </div>
  );
}
