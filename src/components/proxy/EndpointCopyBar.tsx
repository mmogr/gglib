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
  return `http://${host}:${port}/v1`;
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
      <code className="flex-1 bg-surface-elevated p-sm rounded-base text-sm border border-border font-mono truncate">
        {url}
      </code>
      <Button
        variant="ghost"
        size="sm"
        onClick={copy}
        className="bg-primary border-none rounded-base p-sm cursor-pointer text-base text-white transition-all hover:bg-primary-hover hover:scale-105"
        title="Copy URL"
        iconOnly
      >
        <Icon icon={ClipboardCopy} size={14} />
      </Button>
    </div>
  );
};
