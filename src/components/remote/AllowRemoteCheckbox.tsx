import { FC, ReactNode, useState } from 'react';
import { Checkbox } from '../ui/Checkbox';
import { Label, Stack } from '../primitives';
import type { McpServer, McpServerId } from '../../services/transport';

interface AllowRemoteCheckboxProps {
  /** The server whose flag the box shows and sets. */
  server: McpServer;
  /** What the box reads beside it. */
  label: ReactNode;
  /** Set the flag, and refresh whatever lists the server. */
  onToggle: (id: McpServerId, allow: boolean) => Promise<void>;
}

/**
 * Whether paired devices may use one MCP server: the box the MCP panel shows
 * on each row it can open and the Remote popover shows as its checklist, so
 * both set the one flag the same way. A change applies from a device's next
 * run, with no restart. A disabled server is said to offer nothing, and a
 * refused change is said beside the box.
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
      setError(e instanceof Error ? e.message : 'Could not change whether paired devices may use it');
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
