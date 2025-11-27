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

// Export mock for use in tests
export { mockInvoke };

// Reset all mocks before each test
beforeEach(() => {
  vi.clearAllMocks();
});
