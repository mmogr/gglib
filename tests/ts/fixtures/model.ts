/**
 * A library-list model, as `GET /api/models` actually sends one.
 *
 * `GuiModel` has eight fields the old hand-written mirror made optional:
 * `id`, `capabilities`, `tags` and `isServing` are always sent, and
 * `architecture`, `quantization`, `contextLength` and `hfRepoId` are
 * always-present nullables. A fixture naming five keys typechecked only
 * because the mirror was wrong about all eight.
 *
 * The three MoE fields are genuinely optional — they carry
 * `skip_serializing_if`, so a dense model omits them — and so this builder
 * leaves them out. Pass them explicitly to describe a MoE row.
 */
import type { GgufModel } from '../../../src/types';

const BASE: GgufModel = {
  id: 1,
  name: 'Test Model',
  filePath: '/models/test.gguf',
  paramCountB: 7.0,
  architecture: null,
  quantization: null,
  contextLength: null,
  hfRepoId: null,
  // `GuiModel::from_model` formats as `%Y-%m-%d %H:%M:%S` — no `T`, no
  // zone. Nothing in `src/` parses it, but a fixture claiming to be what
  // the wire sends should be.
  addedAt: '2024-01-01 00:00:00',
  tags: [],
  isServing: false,
  capabilities: 0,
};

/** A model row with `overrides` applied over a plausible dense default. */
export function guiModel(overrides: Partial<GgufModel> = {}): GgufModel {
  return { ...BASE, ...overrides };
}
