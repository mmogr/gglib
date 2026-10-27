import { FC } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Icon } from "../ui/Icon";
import { IconButton } from "../ui/IconButton";
import { SecretField, editedValue, type SecretEdit } from "../ui/SecretField";
import { SecretInput } from "../ui/SecretInput";
import { Plus, X } from "lucide-react";
import { Stack, Label } from '../primitives';
import type { McpEnvUpdate } from '../../services/transport';

/**
 * A variable in the editor: one stored server `serverId` keeps, named by its
 * key with its value held and never sent to the page (ADR 0027), or one
 * typed here, known by an `id` of its own while its key is still being
 * typed. A kept variable's key cannot be changed: renaming one is removing
 * it and adding it again.
 */
export type EnvRow =
  | { kept: true; serverId: number; key: string; edit: SecretEdit }
  | { kept: false; id: number; key: string; value: string };

let typedRows = 0;

/** A row to type a variable into: `key` already filled in for a template's. */
export function typedRow(key = ''): EnvRow {
  typedRows += 1;
  return { kept: false, id: typedRows, key, value: '' };
}

/**
 * The row's React key. Never its index: a row removed above another would
 * hand that row's field to the next variable, a value revealed in it
 * included.
 */
const rowKey = (row: EnvRow): string => (row.kept ? `kept:${row.key}` : `typed:${row.id}`);

/**
 * What a save sends for `rows`: a kept variable with its new value, or with
 * none so the stored one stays; a typed one with its key and value. A typed
 * row with no key is left out, and a row removed is not sent, which deletes
 * it.
 */
export function envSent(rows: EnvRow[]): McpEnvUpdate[] {
  return rows.flatMap((row): McpEnvUpdate[] => {
    if (row.kept) {
      const value = editedValue(row.edit);
      return [typeof value === 'string' ? { key: row.key, value } : { key: row.key }];
    }
    return row.key.trim() ? [{ key: row.key, value: row.value }] : [];
  });
}

interface EnvVarManagerProps {
  rows: EnvRow[];
  onAdd: () => void;
  onRemove: (index: number) => void;
  onUpdate: (index: number, row: EnvRow) => void;
  disabled: boolean;
  /** How a kept variable's Show or Copy went. */
  onNotice?: (message: string, kind: 'success' | 'error') => void;
}

export const EnvVarManager: FC<EnvVarManagerProps> = ({
  rows,
  onAdd,
  onRemove,
  onUpdate,
  disabled,
  onNotice,
}) => {
  return (
    <Stack gap="xs">
      <div className="flex justify-between items-center">
        <Label size="sm">Environment Variables</Label>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="px-0 text-xs text-primary hover:text-primary"
          onClick={onAdd}
          disabled={disabled}
        >
          <Icon icon={Plus} size={14} />
          Add variable
        </Button>
      </div>
      {rows.length === 0 ? (
        <p className="text-xs text-text-secondary">
          Add environment variables for API keys and secrets
        </p>
      ) : (
        <div className="flex flex-col gap-sm">
          {rows.map((row, index) => (
            <div key={rowKey(row)} className="flex gap-sm items-center">
              <Input
                type="text"
                className="flex-1 font-mono"
                value={row.key}
                onChange={(e) => {
                  if (!row.kept) onUpdate(index, { ...row, key: e.target.value });
                }}
                readOnly={row.kept}
                aria-label={row.kept ? `${row.key}, kept` : 'Variable name'}
                placeholder="KEY"
                disabled={disabled}
              />
              <div className="flex-[2] min-w-0">
                {row.kept ? (
                  <SecretField
                    id={`env-${rowKey(row)}-value`}
                    label={`${row.key} value`}
                    isSet
                    secret={{ kind: 'mcp_env', server_id: row.serverId, key: row.key }}
                    edit={row.edit}
                    onEdit={(edit) => onUpdate(index, { ...row, edit })}
                    disabled={disabled}
                    onNotice={onNotice}
                  />
                ) : (
                  <SecretInput
                    label={`${row.key || 'New variable'} value`}
                    value={row.value}
                    onChange={(value) => onUpdate(index, { ...row, value })}
                    placeholder="value"
                    disabled={disabled}
                  />
                )}
              </div>
              {/* A bare button inside the form would submit it. */}
              <IconButton
                type="button"
                label={`Remove ${row.key || 'variable'}`}
                size="sm"
                variant="dangerGhost"
                className="h-6 w-6"
                onClick={() => onRemove(index)}
                disabled={disabled}
              >
                <Icon icon={X} size={14} />
              </IconButton>
            </div>
          ))}
        </div>
      )}
    </Stack>
  );
};
