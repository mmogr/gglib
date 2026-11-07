/**
 * The diagnostics place, driven through the real routes: each tab by its
 * address and by a click, `/diagnostics` and a tab that is none going to
 * Live, the sidebar row current on every tab; Live's sections drawn only
 * with something to show, in order, the rest folded into one line, its
 * requests, slots and Clear, its words while the endpoint is off, and one
 * stream with another surface holding it (TX-1); Logs' picker and facts;
 * Benchmarks' segmented modes and Recent Runs; System's checks and
 * Re-check.
 *
 * The transport is a fake; the dashboard stream is played by the test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => [] as unknown[]),
  getDownloadQueue: vi.fn(async () => null),
  listPairedModels: vi.fn(async () => null),
  revealSecret: vi.fn(async (): Promise<string | null> => null),
  listConversations: vi.fn(async (): Promise<unknown[]> => []),
  listRuns: vi.fn(async () => []),
  getRemoteStatus: vi.fn(async () => null),
}));
const dashboard = vi.hoisted(() => ({
  send: null as ((s: unknown) => void) | null,
  opened: vi.fn((_h: string, _p: number, _k: unknown) => {}),
  clear: vi.fn(async (_h: string, _p: number, _k?: string | null) => ({ status: 'ok', message: 'Prompt cache cleared' })),
}));
const reads = vi.hoisted(() => ({
  runs: vi.fn(async (_limit?: number, _offset?: number): Promise<unknown[]> => []),
  trips: vi.fn(async (_days: number): Promise<unknown[]> => []),
  diagnostics: vi.fn(async (): Promise<unknown> => null),
  logs: vi.fn(async (_port: number): Promise<unknown[]> => []),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({ get: vi.fn(async () => null), post: vi.fn(async () => null) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true, llamaCanDownload: true })),
  getRecommendedModel: vi.fn(async () => null),
  getDiagnostics: reads.diagnostics,
  enableFastDownloads: vi.fn(async () => undefined),
  disableFastDownloads: vi.fn(async () => ({ removed: true })),
}));
vi.mock('../../../src/services/transport/api/proxy', () => ({ getLoopGuardTrips: reads.trips }));
vi.mock('../../../src/services/clients/benchmark', () => ({ listBenchmarkRuns: reads.runs }));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
vi.mock('../../../src/services/platform/serverLogs', () => ({
  getServerLogs: reads.logs,
  listenToServerLogs: vi.fn(async () => () => {}),
}));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (h: string, p: number, k: unknown, onSnapshot: (s: unknown) => void) => {
    dashboard.opened(h, p, k);
    dashboard.send = onSnapshot;
    return () => {
      dashboard.send = null;
    };
  },
  clearProxyCache: (h: string, p: number, k?: string | null) => dashboard.clear(h, p, k),
}));
// The modes are their own tests' (benchmarkAbort, tuneStreamEnds, …); here
// each says which mode it is, whether it is the open one, and its models.
vi.mock('../../../src/places/diagnostics/benchmarks/PerfCompareTab', () => ({
  PerfCompareTab: (p: { mode: string; active: boolean; initialModelIds?: number[]; onRunComplete: () => void }) => (
    <div data-testid="perf-compare" data-mode={p.mode} data-active={String(p.active)} data-ids={(p.initialModelIds ?? []).join(',')}>
      <button type="button" onClick={p.onRunComplete}>finish perf</button>
    </div>
  ),
}));
vi.mock('../../../src/places/diagnostics/benchmarks/Tune/TuneTab', () => ({ TuneTab: () => <div data-testid="tune" /> }));
vi.mock('../../../src/places/diagnostics/benchmarks/Agentic/AgenticTab', () => ({ AgenticTab: () => <div data-testid="agentic" /> }));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { acquireDashboard, openDashboardStreams } from '../../../src/services/dashboardRegistry';
import { applyProxyStatus, resetProxyState } from '../../../src/services/proxyRegistry';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { cleanupModelEvents } from '../../../src/services/modelEvents';
import { resetChats } from '../../../src/services/chatRegistry';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';
import type { DashboardSnapshot } from '../../../src/services/transport/types/dashboard';
import {
  activeConnection,
  dashboardSnapshot,
  modelDefectCounts,
  recentRequest,
  residentSlot,
  samplingAudit,
  slotSnapshot,
} from '../fixtures/dashboard';
import { renderApp, renderShell } from './harness';

const UP: ProxyStatus = { running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'none_needed', pinned_model: null };
const DOWN: ProxyStatus = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null };

function renderAt(path: string, status: ProxyStatus = UP) {
  act(() => applyProxyStatus(status));
  return { ...renderApp(path), user: userEvent.setup() };
}

/** Play one dashboard frame, once the place has opened the stream. */
async function frame(overrides: Partial<DashboardSnapshot> = {}) {
  await waitFor(() => expect(dashboard.send).not.toBeNull());
  act(() => dashboard.send?.(dashboardSnapshot(overrides)));
}

const CACHE = {
  disk_enabled: true,
  disk_suppressed_for_model: false,
  ram_budget_mb: 2048,
  ram_state: 'healthy' as const,
  needs_attention: false,
  warnings: [],
  usage: { reporting_requests: 4, unreported_requests: 0, prompt_tokens: 1000, cached_tokens: 710, last_prompt_tokens: null, last_cached_tokens: null },
};

/** A frame in which every section has something. */
const FULL: Partial<DashboardSnapshot> = {
  recent_requests: [recentRequest({ model_name: 'Qwen3-4B' })],
  total_requests: 1,
  admission: { ...dashboardSnapshot().admission, slots: [residentSlot(1, 'Qwen3-4B')] },
  launch: { model_name: 'Qwen3-4B', quantization: 'Q4_K_M', weights_bytes: 2_684_354_560, decisions: [{ label: 'Context', value: '8192', source: 'model author' }] },
  cache: CACHE,
  agent_usage: { ...CACHE.usage, reporting_requests: 2 },
  sampling_audit: samplingAudit({ state: { state: 'blind', reason: 'no slot has reported yet' } }),
  per_model_defects: { 'Qwen3-4B': modelDefectCounts({ requests: 1 }) },
  slots_available: true,
  slots_status: null,
  slots: [slotSnapshot({ id: 0, is_processing: true, n_ctx: 4096, n_prompt_tokens: 1000, next_token: { n_decoded: 24 } })],
};

const TUNE_RUN = {
  id: 41,
  run_type: 'tune',
  status: 'completed',
  model_ids: [1],
  prompt_text: null,
  system_prompt: null,
  config_json: '{}',
  applied_json: JSON.stringify({ verdict: { verdict: 'uncalibrated' } }),
  error: null,
  created_at: '2026-11-07T10:00:00Z',
  completed_at: null,
};
const TRIP_DAY = { day: '2026-11-06', model_name: 'Qwen3-4B', gglib_version: '0.31.0', mode: 'note', scanned: 9, trips: 1, loops: 1, stagnations: 0, sessions: 1 };

const live = () => screen.getByRole('tabpanel', { name: 'Live' });
/** Live's own sections, not a region a panel draws inside one. */
const sectionNames = () =>
  within(live())
    .queryAllByRole('region')
    .filter((r) => r.parentElement?.closest('section')?.getAttribute('aria-label') === 'Diagnostics')
    .map((r) => r.getAttribute('aria-label'));
const fold = () => screen.getByText(/^Nothing yet: /);

let clock = Date.now() + 2e9;
function serve(id: number, name: string, port: number) {
  act(() => ingestServerEvent({ type: 'running', modelId: String(id), port, updatedAt: (clock += 1e6), modelName: name }));
}
function stop(id: number) {
  act(() => ingestServerEvent({ type: 'stopped', modelId: String(id), updatedAt: (clock += 1e6) }));
}

describe('the diagnostics place', () => {
  beforeEach(() => {
    resetChats();
    dashboard.opened.mockClear();
    dashboard.clear.mockClear();
    for (const read of Object.values(reads)) read.mockReset();
    reads.runs.mockImplementation(async () => []);
    reads.trips.mockImplementation(async () => []);
    reads.diagnostics.mockImplementation(async () => DIAGNOSTICS);
    reads.logs.mockImplementation(async () => []);
    transport.revealSecret.mockImplementation(async () => null);
  });
  afterEach(() => {
    stop(1);
    stop(2);
    act(() => resetProxyState());
    cleanupModelEvents();
  });

  describe('its tabs', () => {
    it.each([
      ['/diagnostics/live', 'Live'],
      ['/diagnostics/logs', 'Logs'],
      ['/diagnostics/benchmarks', 'Benchmarks'],
      ['/diagnostics/system', 'System'],
    ])('%s opens %s, selected in the tablist', async (path, name) => {
      renderAt(path);
      const tabs = within(screen.getByRole('tablist', { name: 'Diagnostics' })).getAllByRole('tab');
      expect(tabs.map((t) => t.textContent)).toEqual(['Live', 'Logs', 'Benchmarks', 'System']);
      expect(screen.getByRole('tab', { name, selected: true })).toBeInTheDocument();
      expect(tabs.filter((t) => t.getAttribute('aria-selected') === 'true')).toHaveLength(1);
      expect(await screen.findByRole('tabpanel', { name })).toBeInTheDocument();
    });

    it('/diagnostics and a tab that is none go to Live, in place of the address', async () => {
      const first = renderAt('/diagnostics');
      await waitFor(() => expect(first.at()).toBe('/diagnostics/live'));
      expect(first.router.state.historyAction).toBe('REPLACE');
      first.unmount();

      const second = renderAt('/diagnostics/nope');
      await waitFor(() => expect(second.at()).toBe('/diagnostics/live'));
      expect(screen.getByRole('tab', { name: 'Live', selected: true })).toBeInTheDocument();
    });

    it('a tab picked moves the address, and the address reloaded opens it again', async () => {
      const { user, at, router, unmount } = renderAt('/diagnostics/live');
      await user.click(screen.getByRole('tab', { name: 'System' }));
      expect(at()).toBe('/diagnostics/system');
      expect(router.state.historyAction).toBe('PUSH');
      expect(await screen.findByRole('tabpanel', { name: 'System' })).toBeInTheDocument();
      await waitFor(() => expect(reads.diagnostics).toHaveBeenCalledTimes(1));
      unmount();

      renderAt('/diagnostics/system');
      expect(screen.getByRole('tab', { name: 'System', selected: true })).toBeInTheDocument();
      await waitFor(() => expect(reads.diagnostics).toHaveBeenCalledTimes(2));
    });

    it('the sidebar row is current on every tab, and opens Live', async () => {
      const app = renderShell('/diagnostics/system');
      const row = () => screen.getByRole('button', { name: 'Diagnostics' });
      expect(row()).toHaveAttribute('aria-current', 'page');
      await act(() => app.router.navigate('/diagnostics/logs'));
      expect(row()).toHaveAttribute('aria-current', 'page');
      await act(() => app.router.navigate('/library'));
      expect(row()).not.toHaveAttribute('aria-current');
      await userEvent.setup().click(row());
      expect(app.at()).toBe('/diagnostics/live');
      expect(row()).toHaveAttribute('aria-current', 'page');
    });
  });

  describe('Live', () => {
    it('draws every section that has something, in order', async () => {
      reads.runs.mockImplementation(async () => [TUNE_RUN]);
      reads.trips.mockImplementation(async () => [TRIP_DAY]);
      renderAt('/diagnostics/live');
      await frame(FULL);
      await waitFor(() => expect(sectionNames()).toContain('Loop-guard trips'));
      await waitFor(() => expect(sectionNames()).toContain('Tune activity'));
      expect(sectionNames()).toEqual([
        'Requests',
        'Loaded and queue',
        'Launch decisions',
        'Prompt cache',
        'Agent cache',
        'Sampling readback',
        'Per-model signals',
        'Tune activity',
        'Slots',
        'Loop-guard trips',
      ]);
      expect(screen.queryByText(/^Nothing yet: /)).not.toBeInTheDocument();
      // Blind is said, never folded as agreement.
      expect(within(screen.getByRole('region', { name: 'Sampling readback' })).getByText(/no slot has reported yet/)).toBeInTheDocument();
      expect(within(screen.getByRole('region', { name: 'Tune activity' })).getByText('run #41')).toBeInTheDocument();
      expect(screen.getByRole('region', { name: 'Loaded and queue' })).toHaveTextContent('No second model has been requested yet.');
    });

    it('folds every section with nothing into one line, in their order', async () => {
      renderAt('/diagnostics/live');
      await frame();
      await waitFor(() => expect(reads.trips).toHaveBeenCalled());
      expect(sectionNames()).toEqual([]);
      expect(fold()).toHaveTextContent(
        'Nothing yet: Requests · Loaded and queue · Launch decisions · Prompt cache · Agent cache · Sampling readback · Per-model signals · Tune activity · Slots · Loop-guard trips.',
      );
      expect(fold()).toHaveClass('text-xs');
    });

    it('draws a section as it gets something, and the fold keeps the rest', async () => {
      renderAt('/diagnostics/live');
      await frame({ launch: FULL.launch, slots_available: true, slots_status: null, slots: FULL.slots });
      expect(sectionNames()).toEqual(['Launch decisions', 'Slots']);
      expect(fold()).toHaveTextContent(
        'Nothing yet: Requests · Loaded and queue · Prompt cache · Agent cache · Sampling readback · Per-model signals · Tune activity · Loop-guard trips.',
      );
    });

    it('Requests: in flight with its phase, then every request kept, newest first, and the total', async () => {
      renderAt('/diagnostics/live');
      await frame({
        total_requests: 1204,
        active_connections: [activeConnection({ id: 'c1', model_name: 'Gemma-3-4B-It', phase: 'processing_prompt', prompt_processed: 5, prompt_total: 10 })],
        recent_requests: [0, 1, 2, 3, 4, 5, 6].map((n) => recentRequest({ model_name: `m${n}`, recorded_at_secs: 1_800_000_000 + n * 60 })),
      });
      const requests = screen.getByRole('region', { name: 'Requests' });
      expect(requests).toHaveTextContent('1,204 since the endpoint turned on');
      const flight = within(requests).getByRole('list', { name: 'In flight' });
      expect(flight).toHaveTextContent('Gemma-3-4B-It');
      expect(flight).toHaveTextContent('Processing prompt');
      expect(within(flight).getByRole('progressbar')).toBeInTheDocument();
      const rows = within(within(requests).getByRole('list', { name: 'Recent requests' })).getAllByRole('listitem');
      expect(rows.map((r) => r.children[1].textContent)).toEqual(['m6', 'm5', 'm4', 'm3', 'm2', 'm1', 'm0']);
    });

    it('Slots: a bar per slot with its tokens and percent; with none reported while loaded, the reason', async () => {
      renderAt('/diagnostics/live');
      await frame({ slots_available: true, slots_status: null, slots: [...(FULL.slots ?? []), slotSnapshot({ id: 1, n_ctx: 4096 })] });
      const slots = screen.getByRole('region', { name: 'Slots' });
      expect(slots).toHaveTextContent('Slot 0 · active1,024 of 4,096 tokens · 25%');
      expect(within(slots).getByRole('progressbar', { name: 'Slot 0 context' })).toHaveAttribute('aria-valuenow', '25');
      expect(slots).toHaveTextContent('Slot 1context unknown');
      expect(within(slots).queryByRole('progressbar', { name: 'Slot 1 context' })).not.toBeInTheDocument();

      await frame({ slots_status: 'slots are disabled (--no-slots)', admission: FULL.admission });
      expect(screen.getByRole('region', { name: 'Slots' })).toHaveTextContent('slots are disabled (--no-slots)');
    });

    it('Clear clears the prompt cache with the revealed key, and says so', async () => {
      transport.revealSecret.mockImplementation(async () => 'k3y');
      const { user } = renderAt('/diagnostics/live', { ...UP, key: 'required' });
      await frame({ cache: CACHE });
      const cache = screen.getByRole('region', { name: 'Prompt cache' });
      await user.click(within(cache).getByRole('button', { name: 'Clear' }));
      expect(dashboard.clear).toHaveBeenCalledWith('127.0.0.1', 8080, 'k3y');
      await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent('Prompt cache cleared'));
    });

    it('while the endpoint is off, says so, opens no stream, and still shows the reads', async () => {
      reads.trips.mockImplementation(async () => [TRIP_DAY]);
      renderAt('/diagnostics/live', DOWN);
      expect(screen.getByText('The endpoint is off. Its live numbers show while it is on.')).toBeInTheDocument();
      expect(await screen.findByRole('region', { name: 'Loop-guard trips' })).toBeInTheDocument();
      expect(dashboard.opened).not.toHaveBeenCalled();
    });

    it('before the first frame, says it is reading', async () => {
      renderAt('/diagnostics/live');
      await waitFor(() => expect(dashboard.send).not.toBeNull());
      expect(screen.getByText("Reading the endpoint's numbers…")).toBeInTheDocument();
      await frame();
      expect(screen.queryByText("Reading the endpoint's numbers…")).not.toBeInTheDocument();
    });

    it("opens no second stream beside another surface's (TX-1)", async () => {
      act(() => applyProxyStatus(UP));
      // The endpoint place's hold on the stream: the same target it builds.
      const release = acquireDashboard({ host: '127.0.0.1', port: 8080, apiKey: null });
      try {
        expect(openDashboardStreams()).toBe(1);
        renderApp('/diagnostics/live');
        await waitFor(() => expect(transport.revealSecret).toHaveBeenCalled());
        await frame({ launch: FULL.launch });
        expect(await screen.findByRole('region', { name: 'Launch decisions' })).toBeInTheDocument();
        expect(openDashboardStreams()).toBe(1);
        expect(dashboard.opened).toHaveBeenCalledTimes(1);
      } finally {
        release();
      }
    });
  });

  describe('Logs', () => {
    it('picks among the models loaded now, each with its own log and facts', async () => {
      serve(1, 'Qwen3-4B', 9001);
      serve(2, 'Gemma-3-4B-It', 9002);
      reads.logs.mockImplementation(async (port: number) => [{ timestamp: 1, line: `line from ${port}`, port }]);
      const { user } = renderAt('/diagnostics/logs');
      await frame({
        admission: {
          ...dashboardSnapshot().admission,
          slots: [
            residentSlot(1, 'Qwen3-4B', { inflight: 2, resident_for_secs: 60 }),
            residentSlot(2, 'Gemma-3-4B-It', { slot: 1, is_primary: false, resident_for_secs: 3700 }),
          ],
        },
        slots_available: true,
        slots_status: null,
        slots: FULL.slots,
        recent_requests: [recentRequest({ model_name: 'Qwen3-4B', completion_tokens: 812, generation_ms: 24_000 })],
      });
      const picker = screen.getByRole('combobox', { name: 'Model' });
      expect(within(picker).getAllByRole('option').map((o) => o.textContent)).toEqual(['Qwen3-4B', 'Gemma-3-4B-It']);
      expect(picker).toHaveValue('1');
      const facts = () => screen.getByRole('complementary', { name: 'Facts' });
      const pairs = () => within(facts()).getAllByRole('term').map((t) => `${t.textContent} ${t.nextElementSibling?.textContent}`);
      await waitFor(() => expect(pairs()).toHaveLength(6));
      expect(pairs()).toEqual([
        'Port 9001',
        'Uptime 1m 00s',
        'Context 8,192 tokens',
        'KV use 1,024 of 4,096 tokens · 25%',
        'Writing 34 t/s',
        'Active requests 2',
      ]);
      expect(await screen.findByText('line from 9001')).toBeInTheDocument();
      expect(screen.getByRole('heading', { name: 'Qwen3-4B · Log' })).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();

      await user.selectOptions(picker, '2');
      // Not the primary: its slots are not polled, so no KV use; no rate recorded.
      expect(pairs()).toEqual(['Port 9002', 'Uptime 1h 01m', 'Context 8,192 tokens', 'Active requests 0']);
      expect(await screen.findByText('line from 9002')).toBeInTheDocument();

      stop(2);
      await waitFor(() => expect(picker).toHaveValue('1'));
    });

    it('with the endpoint off, gives the port alone', async () => {
      serve(1, 'Qwen3-4B', 9001);
      renderAt('/diagnostics/logs', DOWN);
      const facts = screen.getByRole('complementary', { name: 'Facts' });
      expect(within(facts).getAllByRole('term').map((t) => t.textContent)).toEqual(['Port']);
    });

    it('with nothing loaded, says when a log shows', () => {
      renderAt('/diagnostics/logs');
      expect(screen.getByText("Nothing is loaded. A model's log shows here while it is loaded.")).toBeInTheDocument();
      expect(screen.queryByRole('combobox', { name: 'Model' })).not.toBeInTheDocument();
    });
  });

  describe('Benchmarks', () => {
    it('Perf first; Compare, Tune and Agentic segmented, perf kept mounted while another is open', async () => {
      const { user } = renderAt('/diagnostics/benchmarks?models=9,11');
      const modes = await screen.findByRole('tablist', { name: 'Benchmark mode' });
      expect(within(modes).getAllByRole('tab').map((t) => t.textContent)).toEqual(['Perf', 'Compare', 'Tune', 'Agentic']);
      const perf = screen.getByTestId('perf-compare');
      expect(perf).toHaveAttribute('data-mode', 'perf');
      expect(perf).toHaveAttribute('data-active', 'true');
      expect(perf).toHaveAttribute('data-ids', '9,11');

      await user.click(within(modes).getByRole('tab', { name: 'Compare' }));
      expect(screen.getByTestId('perf-compare')).toHaveAttribute('data-mode', 'compare');

      await user.click(within(modes).getByRole('tab', { name: 'Tune' }));
      expect(screen.getByTestId('tune')).toBeInTheDocument();
      expect(screen.getByTestId('perf-compare')).toHaveAttribute('data-active', 'false');

      await user.click(within(modes).getByRole('tab', { name: 'Agentic' }));
      expect(screen.getByTestId('agentic')).toBeInTheDocument();
      expect(screen.queryByTestId('tune')).not.toBeInTheDocument();
    });

    it('Recent Runs is read on opening and after a run, with no Refresh', async () => {
      reads.runs.mockImplementation(async () => [{ ...TUNE_RUN, id: 7, run_type: 'perf' }]);
      const { user } = renderAt('/diagnostics/benchmarks');
      const recent = await screen.findByRole('heading', { name: 'Recent Runs' });
      await waitFor(() => expect(screen.getByRole('table')).toHaveTextContent('perf'));
      expect(reads.runs).toHaveBeenCalledTimes(1);
      expect(recent.parentElement).not.toHaveTextContent('Refresh');
      expect(screen.queryByRole('button', { name: 'Refresh' })).not.toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: 'finish perf' }));
      await waitFor(() => expect(reads.runs).toHaveBeenCalledTimes(2));
    });
  });

  describe('System', () => {
    it('shows the checks, and Re-check reads them again', async () => {
      const { user } = renderAt('/diagnostics/system');
      const panel = await screen.findByRole('tabpanel', { name: 'System' });
      await within(panel).findByText('Dependencies');
      for (const heading of ['Dependencies', 'Acceleration', 'Paths', 'Download accelerator']) {
        expect(within(panel).getByRole('heading', { name: heading })).toBeInTheDocument();
      }
      expect(within(panel).getByText('/models')).toBeInTheDocument();
      expect(reads.diagnostics).toHaveBeenCalledTimes(1);

      await user.click(within(panel).getByRole('button', { name: 'Re-check' }));
      await waitFor(() => expect(reads.diagnostics).toHaveBeenCalledTimes(2));
    });
  });
});

const DIAGNOSTICS = {
  dependencies: [{ name: 'cmake', status: 'present', version: '3.28.1', description: 'Build', required: true, installHint: null }],
  paths: {
    dataRoot: '/home/u/.local/share/gglib',
    resourceRoot: '/home/u/.local/share/gglib',
    databasePath: '/home/u/.local/share/gglib/gglib.db',
    llamaServerPath: '/home/u/.local/share/gglib/bin/llama-server',
    sdServerPath: '/home/u/.local/share/gglib/.sd/bin/sd-server',
    modelsDir: '/models',
    modelsSource: 'default',
  },
  acceleration: { detected: 'Metal', detectionError: null },
  fastDownloads: { provisioned: false, envDir: null, legacyPath: false, builder: null, availableBuilder: 'uv', error: null },
};
