/**
 * The one sheet that adds a tool server and edits one: the quick starts
 * (adding only), Name, Command, Arguments, Environment variables with their
 * values held (ADR 0027), Use in chats, and under ▸ More the Working
 * directory, Extra PATH and Start when gglib starts.
 *
 * It offers only what can run: a stdio server. The daemon refuses an SSE
 * one, so there is no connection type to choose and no URL until it can be
 * chosen (ST-10). Edit opens it filled from the stored server, ▸ More open
 * when anything under it is set. A refused save is said in the sheet, in the
 * daemon's words, with what was typed kept.
 *
 * @module places/settings/tools/ServerSheet
 */

import { useState, type FC, type FormEvent, type ReactNode } from 'react';
import { Label, Stack } from '../../../components/primitives';
import { Button } from '../../../components/ui/Button';
import { Checkbox } from '../../../components/ui/Checkbox';
import { Input } from '../../../components/ui/Input';
import { Modal } from '../../../components/ui/Modal';
import { KEEP } from '../../../components/ui/SecretField';
import { useToastContext } from '../../../contexts/ToastContext';
import type { McpServerInfo, NewMcpServer } from '../../../services/transport';
import { formatError } from '../../../utils/errors';
import { Disclosure } from '../Disclosure';
import { EnvVarManager, envSent, typedRow, type EnvRow } from './EnvVarManager';
import { TemplatePicker, type ServerTemplate } from './TemplatePicker';

const FORM_ID = 'tool-server-form';

interface Fields {
  name: string;
  command: string;
  args: string;
  workingDir: string;
  pathExtra: string;
  env: EnvRow[];
  inChats: boolean;
  atStart: boolean;
}

const EMPTY: Fields = { name: '', command: '', args: '', workingDir: '', pathExtra: '', env: [], inChats: true, atStart: false };

/** The fields for `editing`, or a new server's. */
function fieldsOf(editing: McpServerInfo | undefined): Fields {
  if (!editing) return EMPTY;
  const { server } = editing;
  return {
    name: server.name,
    command: server.config.command ?? '',
    args: server.config.args?.join(' ') ?? '',
    workingDir: server.config.working_dir ?? '',
    pathExtra: server.config.path_extra ?? '',
    // The keys alone: a stored value never reaches the page (ADR 0027).
    env: server.env_keys.map((key) => ({ kept: true, serverId: server.id, key, edit: KEEP })),
    inChats: server.enabled,
    atStart: server.lifecycle === 'eager',
  };
}

/** Why `fields` cannot be saved, or `null`. */
function refusal(fields: Fields): string | null {
  if (!fields.name.trim()) return 'A server needs a name';
  if (!fields.command.trim()) return 'A server needs a command';
  if (fields.command.trim().includes(' ')) {
    return 'Command is one executable, such as npx; its arguments go in Arguments';
  }
  return null;
}

/** What a save sends for `fields`. */
export function serverSent(fields: Fields): NewMcpServer {
  const args = fields.args.trim();
  return {
    name: fields.name.trim(),
    enabled: fields.inChats,
    lifecycle: fields.atStart ? 'eager' : 'lazy',
    env: envSent(fields.env),
    config: {
      command: fields.command.trim(),
      args: args ? args.split(/\s+/) : [],
      working_dir: fields.workingDir.trim() || undefined,
      path_extra: fields.pathExtra.trim() || undefined,
    },
  };
}

const Field: FC<{ id: string; label: string; hint?: string; children: ReactNode }> = ({ id, label, hint, children }) => (
  <Stack gap="xs">
    <Label size="sm" htmlFor={id}>
      {label}
    </Label>
    {children}
    {hint && <span className="text-xs text-text-secondary">{hint}</span>}
  </Stack>
);

interface ServerSheetProps {
  /** The server to edit; absent, the sheet adds one. */
  editing?: McpServerInfo;
  onClose: () => void;
  onSave: (server: NewMcpServer) => Promise<void>;
}

export const ServerSheet: FC<ServerSheetProps> = ({ editing, onClose, onSave }) => {
  const { showToast } = useToastContext();
  const [fields, setFields] = useState<Fields>(() => fieldsOf(editing));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<Fields>) => setFields((f) => ({ ...f, ...patch }));
  const setEnv = (change: (rows: EnvRow[]) => EnvRow[]) => setFields((f) => ({ ...f, env: change(f.env) }));

  const pick = (template: ServerTemplate) =>
    set({
      name: template.name,
      command: template.command,
      args: template.args.join(' '),
      env: template.envKeys.map((key) => typedRow(key)),
    });

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const why = refusal(fields);
    setError(why);
    if (why !== null) return;
    setSaving(true);
    try {
      await onSave(serverSent(fields));
    } catch (e) {
      setError(formatError(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={editing ? `Edit ${editing.server.name}` : 'Add a server'}
      size="lg"
      preventClose={saving}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button type="submit" form={FORM_ID} variant="primary" disabled={saving}>
            {saving ? 'Saving…' : editing ? 'Save' : 'Add'}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} onSubmit={(e) => void submit(e)} className="flex flex-col gap-md">
        {!editing && <TemplatePicker onPick={pick} />}
        <Field id="mcp-name" label="Name">
          <Input id="mcp-name" value={fields.name} onChange={(e) => set({ name: e.target.value })} placeholder="My tool server" disabled={saving} />
        </Field>
        <Field id="mcp-command" label="Command" hint="One executable, found on PATH: npx, python3, node.">
          <Input id="mcp-command" value={fields.command} onChange={(e) => set({ command: e.target.value })} placeholder="npx" disabled={saving} />
        </Field>
        <Field id="mcp-args" label="Arguments" hint="Separated by spaces.">
          <Input id="mcp-args" value={fields.args} onChange={(e) => set({ args: e.target.value })} placeholder="-y tavily-mcp" disabled={saving} />
        </Field>
        <EnvVarManager
          rows={fields.env}
          onAdd={() => setEnv((rows) => [...rows, typedRow()])}
          onRemove={(index) => setEnv((rows) => rows.filter((_, i) => i !== index))}
          onUpdate={(index, row) => setEnv((rows) => rows.map((old, i) => (i === index ? row : old)))}
          disabled={saving}
          onNotice={showToast}
        />
        <Checkbox
          checked={fields.inChats}
          onChange={(e) => set({ inChats: e.target.checked })}
          disabled={saving}
          label="Use in chats"
          description="Its tools are offered in chats."
        />
        <Disclosure label="More" defaultOpen={Boolean(fields.workingDir || fields.pathExtra || fields.atStart)}>
          <Field id="mcp-working-dir" label="Working directory" hint="An absolute path, if it needs one.">
            <Input id="mcp-working-dir" value={fields.workingDir} onChange={(e) => set({ workingDir: e.target.value })} placeholder="/absolute/path" disabled={saving} />
          </Field>
          <Field id="mcp-path-extra" label="Extra PATH" hint="Folders added to its PATH, separated by colons.">
            <Input id="mcp-path-extra" value={fields.pathExtra} onChange={(e) => set({ pathExtra: e.target.value })} placeholder="/opt/tools/bin" disabled={saving} />
          </Field>
          <Checkbox
            checked={fields.atStart}
            onChange={(e) => set({ atStart: e.target.checked })}
            disabled={saving}
            label="Start when gglib starts"
            description="Otherwise it starts the first time one of its tools is used."
          />
        </Disclosure>
        {error && (
          <p role="alert" className="m-0 text-sm text-danger">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
};
