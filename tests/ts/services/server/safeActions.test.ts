import { describe, it, expect, vi, beforeEach } from 'vitest';

// Regression guard: safeStopServer must not consult local serverRegistry state.
// If a future change reintroduces `isServerRunning()` gating, this mock will
// throw and the test will fail.
vi.mock('../../../../src/services/serverRegistry', () => ({
  isServerRunning: () => {
    throw new Error('safeStopServer must not consult isServerRunning()');
  },
}));

const transport = vi.hoisted(() => ({
  stopServer: vi.fn(),
}));

vi.mock('../../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../../src/services/transport')>()),
  getTransport: () => transport,
}));

const { stopServer } = transport;

import { safeStopServer } from '../../../../src/services/server/safeActions';
import { TransportError } from '../../../../src/services/transport/errors';

describe('safeStopServer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('always calls stopServer (no client-side gating)', async () => {
    vi.mocked(stopServer).mockResolvedValue(undefined);

    await expect(safeStopServer(123)).resolves.toBeUndefined();

    expect(stopServer).toHaveBeenCalledTimes(1);
    expect(stopServer).toHaveBeenCalledWith(123);
  });

  it('treats NOT_FOUND as idempotent success', async () => {
    vi.mocked(stopServer).mockRejectedValue(new TransportError('NOT_FOUND', 'not found'));

    await expect(safeStopServer(123)).resolves.toBeUndefined();
  });

  it('treats CONFLICT as idempotent success', async () => {
    vi.mocked(stopServer).mockRejectedValue(new TransportError('CONFLICT', 'already stopped'));

    await expect(safeStopServer(123)).resolves.toBeUndefined();
  });

  it('rethrows unexpected errors', async () => {
    vi.mocked(stopServer).mockRejectedValue(new TransportError('INTERNAL', 'boom'));

    await expect(safeStopServer(123)).rejects.toMatchObject({
      name: 'TransportError',
      code: 'INTERNAL',
    });
  });
});
