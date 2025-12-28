import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import {
  normalizeServerEventFromAppEvent,
  normalizeServerEventFromNamedEvent,
} from '../../../../src/services/serverEvents.normalize';

describe('serverEvents.normalize', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2025-01-01T00:00:00.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('normalizes server_snapshot with started_at seconds -> updatedAt ms', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_snapshot',
      servers: [
        {
          modelId: 1,
          modelName: 'M',
          port: 8080,
          started_at: 1_700_000_000,
          healthy: true,
        },
      ],
    });

    expect(evt).toEqual({
      type: 'snapshot',
      servers: [
        {
          modelId: '1',
          status: 'running',
          port: 8080,
          updatedAt: 1_700_000_000_000,
        },
      ],
    });
  });

  it('normalizes server_started into running with deterministic updatedAt', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_started',
      modelId: 123,
      modelName: 'TestModel',
      port: 9000,
    });

    expect(evt).toMatchObject({
      type: 'running',
      modelId: '123',
      port: 9000,
      updatedAt: Date.now(),
    });
  });

  it('normalizes server_stopped into stopped', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_stopped',
      modelId: 123,
      modelName: 'TestModel',
    });

    expect(evt).toMatchObject({
      type: 'stopped',
      modelId: '123',
      updatedAt: Date.now(),
    });
  });

  it('ignores server_error when modelId is missing', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_error',
      modelId: null,
      modelName: 'TestModel',
      error: 'boom',
    });

    expect(evt).toBeNull();
  });

  it('normalizes server_health_changed using timestamp (ms)', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_health_changed',
      serverId: 99,
      modelId: 7,
      status: { status: 'healthy' },
      detail: 'ok',
      timestamp: 1_700_000_000_123,
    });

    expect(evt).toEqual({
      type: 'server_health_changed',
      modelId: '7',
      status: { status: 'healthy' },
      detail: 'ok',
      updatedAt: 1_700_000_000_123,
    });
  });

  it('named-event path matches app-event path for snapshot', () => {
    const payload = {
      type: 'server_snapshot',
      servers: [{ modelId: 1, port: 8080, started_at: 1_700_000_000 }],
    };

    const a = normalizeServerEventFromAppEvent(payload);
    const b = normalizeServerEventFromNamedEvent('server:snapshot', payload);

    expect(b).toEqual(a);
  });
});
