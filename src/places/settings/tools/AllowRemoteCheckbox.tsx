import { FC, ReactNode, useState } from 'react';
import { Checkbox } from '../../../components/ui/Checkbox';
import { Label, Stack } from '../../../components/primitives';
import type { McpServer, McpServerId } from '../../../services/transport';
import { formatError } from '../../../utils/errors';

interface AllowRemoteCheckboxProps {
  /** The server whose flag the box shows and sets. */
  server: McpServer;
  /** What the box reads beside it. */
  label: ReactNode;
  /** Set the flag, and refresh whatever lists the server. */
  onToggle: (id: McpServerId, allow: boolean) => Promise<void>;
}

/**
 * Whether paired devices may use one MCP server: "Devices may use it" in an
 * opened tool server row that can be opened to them, the one place that flag
 * is set (ADR 0025). A change applies
 * from a device's next run, with no restart. A disabled server is said to
 * offer nothing, and a refused change is said beside the box.
 */
export const AllowRemoteCheckbox: FC<AllowRemoteCheckboxProps> = ({ server, label, onToggle }) => {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggle = async (allow: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await onToggle(server.id, allow);
    } catch (e) {
      setError(formatError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Stack gap="xs">
      <Checkbox
        checked={server.allow_remote}
        disabled={busy}
        onChange={(e) => void toggle(e.target.checked)}
        label={label}
        description={server.enabled ? undefined : 'Disabled, so it offers no tools'}
      />
      {error && (
        <Label size="xs" className="text-danger">
          {error}
        </Label>
      )}
    </Stack>
  );
};
