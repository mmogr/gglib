/**
 * Vitest setup file for TypeScript tests.
 * 
 * This file runs before each test file and sets up:
 * - DOM testing utilities from @testing-library/jest-dom
 * - Tauri API mocks for testing components that use Tauri
 */

import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri's invoke API
const mockInvoke = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}));

// Mock Tauri's dialog plugin
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
  save: vi.fn(),
  message: vi.fn(),
  ask: vi.fn(),
  confirm: vi.fn(),
}));

// Mock EventSource for SSE tests (not available in jsdom)
class MockEventSource {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSED = 2;

  url: string;
  readyState: number = MockEventSource.CONNECTING;
  onopen: ((event: Event) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  private listeners: Map<string, ((event: MessageEvent) => void)[]> = new Map();

  constructor(url: string) {
    this.url = url;
    // Simulate async connection
    setTimeout(() => {
      this.readyState = MockEventSource.OPEN;
      if (this.onopen) {
        this.onopen(new Event('open'));
      }
    }, 0);
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    if (!this.listeners.has(type)) {
      this.listeners.set(type, []);
    }
    this.listeners.get(type)!.push(listener);
  }

  removeEventListener(type: string, listener: (event: MessageEvent) => void) {
    const listeners = this.listeners.get(type);
    if (listeners) {
      const index = listeners.indexOf(listener);
      if (index !== -1) {
        listeners.splice(index, 1);
      }
    }
  }

  close() {
    this.readyState = MockEventSource.CLOSED;
  }

  // Helper for tests to emit events
  _emit(type: string, data: unknown) {
    const event = new MessageEvent(type, { data: JSON.stringify(data) });
    const listeners = this.listeners.get(type);
    if (listeners) {
      listeners.forEach(listener => listener(event));
    }
    if (type === 'message' && this.onmessage) {
      this.onmessage(event);
    }
  }
}

// @ts-expect-error - Mock EventSource for jsdom
globalThis.EventSource = MockEventSource;

// Export mock for use in tests
export { mockInvoke, MockEventSource };

// Reset all mocks before each test
beforeEach(() => {
  vi.clearAllMocks();
});
