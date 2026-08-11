/**
 * TrayPanel — the popover behind the system tray icon.
 *
 * Answers the two questions the tray exists for: is the endpoint up, and what
 * is it doing. Everything it renders comes from `components/proxy/`, so it
 * cannot drift from the in-app dashboard.
 *
 * Window-level actions (open gglib, preferences, quit) are deliberately absent:
 * they live on the native tray menu, handled in Rust. Keeping them there means
 * this panel needs no Tauri IPC at all — it talks to the same HTTP API as
 * every other surface, which also makes it testable without mocking Tauri.
 *
 * @module pages/TrayPanel
 */

import { FC, useCallback, useEffect, useState } from 'react';
import {
  EndpointCopyBar,
  ProxyMetricsGrid,
  ProxyStatusPill,
  ProxyToggleButton,
} from '../components/proxy';
import { useProxyDashboard } from '../hooks/useProxyDashboard';
import { useProxyState } from '../services/proxyRegistry';
import { initProxyEvents, cleanupProxyEvents } from '../services/proxyEvents';
import { getTransport } from '../services/transport';
import { appLogger } from '../services/platform';

/** Host the desktop app's proxy is always bound to. */
const PROXY_HOST = '127.0.0.1';

/** How long the "copied" confirmation stays up, in ms. */
const COPIED_FEEDBACK_MS = 1500;

/** Longest error detail the popover shows before trimming. */
const MAX_ERROR_CHARS = 160;

/**
 * Reduce a thrown value to something worth showing in the popover.
 *
 * The detail earns its space: a bare "Could not start the proxy" makes a
 * refused connection look exactly like a port conflict or a bad config, which
 * is the difference between a bug in the app and a port already in use.
 */
function describeError(err: unknown): string {
  const text = (err instanceof Error ? err.message : String(err)).trim();

  if (!text) {
    return 'No further detail was reported.';
  }

  return text.length > MAX_ERROR_CHARS ? `${text.slice(0, MAX_ERROR_CHARS - 1)}…` : text;
}

export const TrayPanel: FC = () => {
  const proxy = useProxyState();
  const [proxyApiKey, setProxyApiKey] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [pinnedModel, setPinnedModel] = useState<string | null>(null);

  // The pin only travels on the status response, not the registry events.
  useEffect(() => {
    if (!proxy.running) {
      setPinnedModel(null);
      return;
    }
    let cancelled = false;
    void getTransport()
      .getProxyStatus()
      .then((s) => {
        if (!cancelled) setPinnedModel(s.pinned_model ?? null);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [proxy.running]);

  // The panel is its own window, so it needs its own subscription — the main
  // window's may not exist yet, or at all.
  useEffect(() => {
    initProxyEvents();
    return cleanupProxyEvents;
  }, []);

  // Read the proxy's key straight off the transport rather than through
  // `useSettings`: this entry point has none of `App`'s providers by design,
  // and one string does not justify introducing one. A failure here leaves the
  // key null, which is correct for the unauthenticated case and merely leaves
  // the dashboard blank for the other — the panel's start/stop still work.
  useEffect(() => {
    let cancelled = false;
    void getTransport()
      .getSettings()
      .then((settings) => {
        if (!cancelled) setProxyApiKey(settings.proxyApiKey ?? null);
      })
      .catch((err) => {
        appLogger.error('service.server', 'Tray panel could not read the proxy API key', { err });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Only stream while the proxy is up; `null` keeps the hook idle.
  const { snapshot, connected } = useProxyDashboard({
    host: PROXY_HOST,
    port: proxy.running ? proxy.port : null,
    apiKey: proxyApiKey,
  });

  useEffect(() => {
    if (!copied) return undefined;
    const timer = setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  const run = useCallback(async (action: () => Promise<unknown>, verb: string) => {
    setPending(true);
    setError(null);
    try {
      await action();
    } catch (err) {
      // The popover has no toast host, so failures are shown in place.
      setError(`Could not ${verb} the proxy. ${describeError(err)}`);
      appLogger.error('component.tray', `Failed to ${verb} proxy`, { error: err });
    } finally {
      setPending(false);
    }
  }, []);

  const handleStart = useCallback(
    () => run(() => getTransport().startProxy({}), 'start'),
    [run],
  );
  const handleStop = useCallback(() => run(() => getTransport().stopProxy(), 'stop'), [run]);

  return (
    <div className="flex flex-col h-screen bg-background text-text overflow-hidden tabular-nums">
      <header className="flex items-center justify-between px-base py-md border-b border-border-light shrink-0">
        <span className="text-sm font-semibold">gglib</span>
        <ProxyStatusPill running={proxy.running} />
      </header>

      <div className="flex-1 overflow-y-auto px-base py-md flex flex-col gap-md">
        {proxy.running && proxy.port !== null ? (
          <>
            <div className="flex flex-col gap-xs">
              <EndpointCopyBar
                host={PROXY_HOST}
                port={proxy.port}
                onCopied={() => setCopied(true)}
              />
              <span className="text-xs text-text-muted h-4">
                {copied ? 'Copied to clipboard' : connected ? 'Live' : 'Connecting…'}
              </span>
              {pinnedModel && (
                <span className="text-xs text-text-muted">
                  Pinned to <span className="font-mono text-text-secondary">{pinnedModel}</span>
                </span>
              )}
            </div>

            <ProxyMetricsGrid snapshot={snapshot} compact />
          </>
        ) : (
          <p className="text-sm text-text-muted">
            The proxy is stopped. Start it to expose your models on an OpenAI-compatible endpoint.
          </p>
        )}

        {error && (
          <p className="text-sm text-danger break-words" role="alert">
            {error}
          </p>
        )}
      </div>

      <footer className="px-base py-md border-t border-border-light shrink-0 flex flex-col gap-sm">
        <ProxyToggleButton
          running={proxy.running}
          pending={pending}
          onStart={handleStart}
          onStop={handleStop}
        />
        <span className="text-xs text-text-muted text-center">
          Right-click the tray icon for more options
        </span>
      </footer>
    </div>
  );
};

export default TrayPanel;
