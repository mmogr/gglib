import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import {
  normalizeServerEventFromAppEvent,
  normalizeServerSnapshotFromList,
} from '../../../../src/services/serverEvents.normalize';
import { MOCK_PROXY_PORT, MOCK_BASE_PORT } from '../../fixtures/ports';
import type { ServerInfo } from '../../../../src/types';

describe('serverEvents.normalize', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2025-01-01T00:00:00.000Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('normalizes server_snapshot with startedAt seconds -> updatedAt ms', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_snapshot',
      servers: [
        {
          modelId: 1,
          modelName: 'M',
          port: MOCK_PROXY_PORT,
          startedAt: 1_700_000_000,
          healthy: true,
        },
      ],
    });

    expect(evt).toEqual({
      type: 'snapshot',
      servers: [
        {
          modelId: '1',
          modelName: 'M',
          status: 'running',
          port: MOCK_PROXY_PORT,
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
      port: MOCK_BASE_PORT,
    });

    expect(evt).toMatchObject({
      type: 'running',
      modelId: '123',
      port: MOCK_BASE_PORT,
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

  it('normalizes server_error into crashed with the structured error envelope', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_error',
      modelId: 123,
      modelName: 'TestModel',
      error: { message: 'model is loading, try again', type: 'service_unavailable', retryable: true },
    });

    expect(evt).toMatchObject({
      type: 'crashed',
      modelId: '123',
      error: { message: 'model is loading, try again', type: 'service_unavailable', retryable: true },
    });
  });

  it('drops a malformed server_error error payload instead of throwing', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_error',
      modelId: 123,
      modelName: 'TestModel',
      error: 'legacy plain string',
    });

    expect(evt).toMatchObject({ type: 'crashed', modelId: '123' });
    expect((evt as { error?: unknown }).error).toBeUndefined();
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

  /**
   * The hydration path, and the reason there are two entry points.
   *
   * `GET /api/servers` reports the same running servers as the SSE snapshot,
   * in a different shape: snake_case keys, a `pid` the registry has no use
   * for, and no `healthy`. It is the current schema, not a legacy one — this
   * test used to claim otherwise while being the only cover for the branch
   * that made startup hydration work.
   */
  it('normalizes the REST server list, which is snake_case', () => {
    const evt = normalizeServerSnapshotFromList([
      {
        model_id: 4,
        model_name: 'SnakeModel',
        pid: 4242,
        port: MOCK_BASE_PORT,
        started_at: 1_700_000_000,
      },
    ]);

    expect(evt).toEqual({
      type: 'snapshot',
      servers: [
        {
          modelId: '4',
          modelName: 'SnakeModel',
          status: 'running',
          port: MOCK_BASE_PORT,
          updatedAt: 1_700_000_000_000,
        },
      ],
    });
  });

  /**
   * The array is typed, but the fetch producing it is an unchecked cast, and
   * the caller swallows rejections — so a null row throwing on property
   * access would cost the whole hydration rather than itself.
   */
  it('drops a malformed REST entry without losing the snapshot', () => {
    const evt = normalizeServerSnapshotFromList([
      null as unknown as ServerInfo,
      { model_id: 9, model_name: 'Good', pid: null, port: 2, started_at: 1_700_000_000 },
    ]);

    expect(evt.servers).toHaveLength(1);
    expect(evt.servers[0]).toMatchObject({ modelId: '9' });
  });

  it('drops REST entries whose id will not coerce, keeping the rest', () => {
    const evt = normalizeServerSnapshotFromList([
      { model_id: NaN, model_name: 'Bad', pid: null, port: 1, started_at: 1_700_000_000 },
      { model_id: 9, model_name: 'Good', pid: null, port: 2, started_at: 1_700_000_000 },
    ]);

    expect(evt.servers).toHaveLength(1);
    expect(evt.servers[0]).toMatchObject({ modelId: '9', modelName: 'Good' });
  });

  /**
   * The SSE snapshot is camelCase throughout — `ServerSnapshotEntry` carries
   * `rename_all = "camelCase"`, so `startedAt` is the only spelling that
   * arrives on this path.
   */
  it('no longer accepts snake_case on the SSE snapshot path', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_snapshot',
      servers: [{ model_id: 4, model_name: 'SnakeModel', port: MOCK_BASE_PORT, started_at: 1 }],
    });

    expect(evt).toEqual({ type: 'snapshot', servers: [] });
  });

  it('drops snapshot entries whose id is missing or non-numeric', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_snapshot',
      servers: [
        { port: MOCK_BASE_PORT }, // no id at all — would stringify to "undefined"
        { modelId: 'abc', port: MOCK_BASE_PORT + 1 },
        { modelId: 5, port: MOCK_BASE_PORT + 2 },
      ],
    });

    expect(evt).toMatchObject({
      type: 'snapshot',
      servers: [{ modelId: '5', port: MOCK_BASE_PORT + 2 }],
    });
  });

  it('ignores lifecycle events with non-numeric model ids', () => {
    const evt = normalizeServerEventFromAppEvent({
      type: 'server_started',
      modelId: 'undefined',
      port: MOCK_BASE_PORT,
    });

    expect(evt).toBeNull();
  });

  it('ignores an event type it does not know', () => {
    expect(normalizeServerEventFromAppEvent({ type: 'server:snapshot' })).toBeNull();
    expect(normalizeServerEventFromAppEvent({ type: 'download_progress' })).toBeNull();
  });
});
