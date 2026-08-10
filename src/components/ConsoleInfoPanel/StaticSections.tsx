import type { FC } from 'react';
import { Copy } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { Stack } from '../primitives';

interface ServerInfoProps {
  serverPort: number;
  uptime: string;
  contextLength?: number;
}

/** Port, uptime, and context-size facts about the running server. */
export const ServerInfoSection: FC<ServerInfoProps> = ({ serverPort, uptime, contextLength }) => (
  <section className="flex flex-col gap-sm">
    <h3 className="m-0 text-sm font-semibold text-text">Server Info</h3>
    <Stack gap="xs">
      <div className="flex justify-between items-center gap-sm py-xs">
        <span className="text-sm text-text-muted">Port</span>
        <span className="text-sm text-text flex items-center gap-xs [&_code]:bg-background [&_code]:py-[2px] [&_code]:px-[6px] [&_code]:rounded-xs [&_code]:font-mono [&_code]:text-xs">
          <code>{serverPort}</code>
          <Button
            iconOnly
            size="sm"
            variant="ghost"
            onClick={() => navigator.clipboard.writeText(`http://127.0.0.1:${serverPort}`)}
            title="Copy server URL"
          >
            <Icon icon={Copy} size={14} />
          </Button>
        </span>
      </div>
      <div className="flex justify-between items-center gap-sm py-xs">
        <span className="text-sm text-text-muted">Uptime</span>
        <span className="text-sm text-text font-mono tabular-nums">{uptime}</span>
      </div>
      {contextLength && (
        <div className="flex justify-between items-center gap-sm py-xs">
          <span className="text-sm text-text-muted">Context Size</span>
          <span className="text-sm text-text font-mono tabular-nums">
            {contextLength.toLocaleString()} tokens
          </span>
        </div>
      )}
    </Stack>
  </section>
);

const ENDPOINTS = [
  { route: 'POST /v1/chat/completions', description: 'OpenAI-compatible chat' },
  { route: 'POST /v1/completions', description: 'Text completion' },
  { route: 'GET /health', description: 'Health check' },
] as const;

/** The static list of API endpoints the server exposes. */
export const ApiEndpointsSection: FC = () => (
  <section className="flex flex-col gap-sm">
    <h3 className="m-0 text-sm font-semibold text-text">API Endpoints</h3>
    <Stack gap="xs">
      {ENDPOINTS.map(({ route, description }) => (
        <div
          key={route}
          className="flex flex-col gap-[2px] py-xs px-sm bg-background rounded-sm [&_code]:font-mono [&_code]:text-xs [&_code]:text-text"
        >
          <code>{route}</code>
          <span className="text-2xs text-text-muted">{description}</span>
        </div>
      ))}
    </Stack>
  </section>
);
