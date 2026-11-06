import { FC, useCallback } from 'react';
import { ClipboardCopy } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';

interface EndpointCopyBarProps {
  host: string;
  port: number;
  /**
   * Called after the URL reaches the clipboard. The main window raises a
   * toast; the tray popover has no toast host and shows inline feedback
   * instead, so the confirmation is the caller's to make.
   */
  onCopied?: (url: string) => void;
}

/** Build the OpenAI-compatible base URL clients should be pointed at. */
export function proxyEndpointUrl(host: string, port: number): string {
  // An IPv6 address needs brackets in a URL; one already bracketed, or a name, does not.
  const authority = host.includes(':') && !host.startsWith('[') ? `[${host}]` : host;
  return `http://${authority}:${port}/v1`;
}

/**
 * Where this app reaches an endpoint bound at `bound` (the stored
 * `proxyHost`; `null` is loopback): a wildcard address, `0.0.0.0` or `::`, on
 * loopback, since that is nowhere a client can send; any other address, a
 * loopback or one interface's, on itself, since nothing listens on loopback
 * then. The daemon's own dial rule; its status says the reached address once
 * #1768 lands, and this reader goes with it.
 */
export function reachableHost(bound: string | null): string {
  return bound === null || bound === '' || bound === '0.0.0.0' || bound === '::' ? '127.0.0.1' : bound;
}

/**
 * The proxy's endpoint URL with a copy button.
 *
 * This is the one string a user actually needs out of gglib when wiring up a
 * client, so it renders identically wherever it appears.
 */
export const EndpointCopyBar: FC<EndpointCopyBarProps> = ({ host, port, onCopied }) => {
  const url = proxyEndpointUrl(host, port);

  const copy = useCallback(() => {
    void navigator.clipboard.writeText(url);
    onCopied?.(url);
  }, [url, onCopied]);

  return (
    <div className="flex gap-sm items-center">
      <code className="flex-1 bg-surface-elevated p-sm rounded-base text-sm font-mono truncate">
        {url}
      </code>
      <Button variant="ghost" size="sm" onClick={copy} title="Copy URL" iconOnly>
        <Icon icon={ClipboardCopy} size={14} />
      </Button>
    </div>
  );
};
