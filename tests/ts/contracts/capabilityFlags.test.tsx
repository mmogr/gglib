/**
 * The capability flags are named once, in the table `gglib model
 * capabilities` and `gglib model inspect` both print from. `gglib-cli`'s
 * capability-flags tests write that table to
 * `contracts/models/capability_flags.json`, each flag with its bit as the
 * core declares it and its key in the PATCH body, and fail there when the
 * file goes stale. The inspector shows the same four: each under that key,
 * which is the camelCase of its name, on that bit, and labelled with the name
 * as words.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { InspectorCapabilities } from '../../../src/components/ModelInspectorPanel/components/InspectorCapabilities';
import { CAPABILITY_FLAGS } from '../../../src/types';

import { rust } from './rustSource';

vi.mock('../../../src/services/transport/api/models/local', () => ({
  setModelCapabilities: vi.fn(),
}));

/** `supports-system-role` as `supportsSystemRole`. */
const camel = (name: string): string => name.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());

/** `supports-system-role` as `Supports system role`. */
const words = (name: string): string => name[0].toUpperCase() + name.slice(1).replaceAll('-', ' ');

/** The CLI's table, in its order: each flag's name, its bit, and its key in the PATCH body. */
const TABLE = JSON.parse(rust('contracts/models/capability_flags.json')) as {
  name: string;
  bit: number;
  field: string;
}[];

describe('the capability flags are the same four on every surface', () => {
  it('names each flag by its request key, on the bit the core gives it, in the order of the table', () => {
    expect(CAPABILITY_FLAGS).toStrictEqual(Object.fromEntries(TABLE.map((row) => [row.field, row.bit])));
    expect(Object.keys(CAPABILITY_FLAGS)).toEqual(TABLE.map((row) => row.field));
  });

  it('keys each flag by its name in camelCase', () => {
    expect(TABLE.map((row) => row.field)).toEqual(TABLE.map((row) => camel(row.name)));
  });

  it('labels each box with the flag name as words', () => {
    render(<InspectorCapabilities modelId={7} capabilities={0} onChanged={vi.fn()} onError={vi.fn()} />);

    const labels = screen.getAllByRole('checkbox').map((box) => box.closest('label')?.textContent ?? '');
    expect(labels).toHaveLength(TABLE.length);
    TABLE.forEach((row, index) => expect(labels[index].startsWith(words(row.name))).toBe(true));
  });
});
