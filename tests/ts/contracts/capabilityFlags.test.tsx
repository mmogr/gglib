/**
 * The capability flags are named once, in the table `gglib model
 * capabilities` and `gglib model inspect` both print from. The inspector
 * shows the same four: each under the camelCase of its name, which is the key
 * the PATCH body carries, on the bit the core gives it, and labelled with the
 * name as words.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { InspectorCapabilities } from '../../../src/components/ModelInspectorPanel/components/InspectorCapabilities';
import { CAPABILITY_FLAGS } from '../../../src/types';

import { rust, withoutComments } from './rustSource';

vi.mock('../../../src/services/transport/api/models/local', () => ({
  setModelCapabilities: vi.fn(),
}));

/** `supports-system-role` as `supportsSystemRole`. */
const camel = (name: string): string => name.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());

/** `supports-system-role` as `Supports system role`. */
const words = (name: string): string => name[0].toUpperCase() + name.slice(1).replaceAll('-', ' ');

/** The CLI's table: each flag's name, the constant its bit is, and its request field. */
const TABLE = [
  ...withoutComments(rust('crates/gglib-cli/src/presentation/capability_flags.rs')).matchAll(
    /name: "([a-z-]+)",\s*bit: ModelCapabilities::([A-Z_]+),\s*field: \|request\| &mut request\.([a-z_]+),/g,
  ),
].map(([, name, constant, field]) => ({ name, constant, field }));

/** The value of one `ModelCapabilities` constant, as the core declares it. */
function bit(constant: string): number {
  const declared = withoutComments(rust('crates/gglib-core/src/domain/capabilities.rs')).match(
    new RegExp(String.raw`const ${constant}\s*=\s*0b([01_]+);`),
  );
  if (!declared) throw new Error(`no ModelCapabilities::${constant}`);
  return parseInt(declared[1].replaceAll('_', ''), 2);
}

describe('the capability flags are the same four on every surface', () => {
  it('reads four rows from the one table', () => {
    expect(TABLE.map((row) => row.name)).toEqual([
      'supports-system-role',
      'requires-strict-turns',
      'supports-tool-calls',
      'supports-reasoning',
    ]);
  });

  it('names each flag as the table does, on the bit the core gives it', () => {
    expect(CAPABILITY_FLAGS).toStrictEqual(
      Object.fromEntries(TABLE.map((row) => [camel(row.name), bit(row.constant)])),
    );
    expect(Object.keys(CAPABILITY_FLAGS)).toEqual(TABLE.map((row) => camel(row.name)));
  });

  it('pairs each name with the request field and the constant of the same name', () => {
    for (const row of TABLE) {
      expect(row.field).toBe(row.name.replaceAll('-', '_'));
      expect(row.constant).toBe(row.field.toUpperCase());
    }
  });

  it('labels each box with the flag name as words', () => {
    render(<InspectorCapabilities modelId={7} capabilities={0} onChanged={vi.fn()} onError={vi.fn()} />);

    const labels = screen.getAllByRole('checkbox').map((box) => box.closest('label')?.textContent ?? '');
    expect(labels).toHaveLength(TABLE.length);
    TABLE.forEach((row, index) => expect(labels[index].startsWith(words(row.name))).toBe(true));
  });
});
