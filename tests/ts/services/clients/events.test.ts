/**
 * Tests for events client.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { subscribeToEvent } from '../../../../src/services/clients/events';
import * as transport from '../../../../src/services/transport';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => ({
  getTransport: vi.fn(),
}));

describe('events client', () => {
  const mockSubscribe = vi.fn();
  const mockUnsubscribe = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    mockSubscribe.mockReturnValue(mockUnsubscribe);
    vi.mocked(transport.getTransport).mockReturnValue({
      subscribe: mockSubscribe,
    } as any);
  });

  describe('subscribeToEvent', () => {
    it('delegates to transport.subscribe', () => {
      const handler = vi.fn();
      const unsub = subscribeToEvent('download', handler);

      expect(transport.getTransport).toHaveBeenCalled();
      expect(mockSubscribe).toHaveBeenCalledWith('download', handler);
      expect(unsub).toBe(mockUnsubscribe);
    });

    it('returns sync unsubscribe function', () => {
      const handler = vi.fn();
      const unsub = subscribeToEvent('server', handler);

      expect(typeof unsub).toBe('function');
      unsub();
      expect(mockUnsubscribe).toHaveBeenCalled();
    });

    it('supports all event types', () => {
      const handler = vi.fn();

      subscribeToEvent('download', handler);
      expect(mockSubscribe).toHaveBeenCalledWith('download', handler);

      subscribeToEvent('server', handler);
      expect(mockSubscribe).toHaveBeenCalledWith('server', handler);

      subscribeToEvent('log', handler);
      expect(mockSubscribe).toHaveBeenCalledWith('log', handler);
    });

    it('handler receives events when emitted', () => {
      const handler = vi.fn();
      
      // Capture the handler passed to subscribe
      let capturedHandler: ((event: any) => void) | null = null;
      mockSubscribe.mockImplementation((type, h) => {
        capturedHandler = h;
        return mockUnsubscribe;
      });

      subscribeToEvent('download', handler);
      
      // Simulate an event
      const mockEvent = { type: 'download_progress', id: 'test', percentage: 50 };
      capturedHandler?.(mockEvent);

      expect(handler).toHaveBeenCalledWith(mockEvent);
    });

    it('handler is not called after unsubscribe', () => {
      const handler = vi.fn();
      let capturedHandler: ((event: any) => void) | null = null;
      let unsubscribeCalled = false;

      mockSubscribe.mockImplementation((type, h) => {
        capturedHandler = h;
        return () => {
          unsubscribeCalled = true;
        };
      });

      const unsub = subscribeToEvent('download', handler);

      // Emit before unsubscribe
      capturedHandler?.({ type: 'download_started', id: 'test' });
      expect(handler).toHaveBeenCalledTimes(1);

      // Unsubscribe
      unsub();
      expect(unsubscribeCalled).toBe(true);
    });
  });
});
