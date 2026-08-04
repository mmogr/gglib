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

export const TrayPanel: FC = () => {
  const proxy = useProxyState();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  // The panel is its own window, so it needs its own subscription — the main
  // window's may not exist yet, or at all.
  useEffect(() => {
    initProxyEvents();
    return cleanupProxyEvents;
  }, []);

  // Only stream while the proxy is up; `null` keeps the hook idle.
  const { snapshot, connected } = useProxyDashboard({
    host: PROXY_HOST,
    port: proxy.running ? proxy.port : null,
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
      setError(`Could not ${verb} the proxy.`);
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
    <div className="flex flex-col h-screen bg-background text-text overflow-hidden">
      <header className="flex items-center justify-between px-base py-md border-b border-border shrink-0">
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
            </div>

            <ProxyMetricsGrid snapshot={snapshot} compact />
          </>
        ) : (
          <p className="text-sm text-text-muted">
            The proxy is stopped. Start it to expose your models on an OpenAI-compatible endpoint.
          </p>
        )}

        {error && (
          <p className="text-sm text-danger" role="alert">
            {error}
          </p>
        )}
      </div>

      <footer className="px-base py-md border-t border-border shrink-0 flex flex-col gap-sm">
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
