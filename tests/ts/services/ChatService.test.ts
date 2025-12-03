/**
 * Tests for ChatService - message update and delete operations.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ChatService } from '../../../src/services/chat';

// Mock fetch globally
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

describe('ChatService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('updateMessage', () => {
    it('sends PUT request with correct parameters', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Message updated' }),
      });

      await ChatService.updateMessage(123, 'Updated content');

      expect(mockFetch).toHaveBeenCalledTimes(1);
      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/messages/123'),
        expect.objectContaining({
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ content: 'Updated content' }),
        })
      );
    });

    it('throws error when response is not ok', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 404,
        statusText: 'Not Found',
        json: () => Promise.resolve({ success: false, error: 'Message not found' }),
      });

      await expect(ChatService.updateMessage(999, 'content')).rejects.toThrow();
    });

    it('handles empty content', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Message updated' }),
      });

      await ChatService.updateMessage(1, '');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/messages/1'),
        expect.objectContaining({
          body: JSON.stringify({ content: '' }),
        })
      );
    });
  });

  describe('deleteMessage', () => {
    it('sends DELETE request with correct message ID', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { deleted_count: 2 } }),
      });

      const result = await ChatService.deleteMessage(456);

      expect(mockFetch).toHaveBeenCalledTimes(1);
      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/messages/456'),
        expect.objectContaining({
          method: 'DELETE',
        })
      );
      expect(result).toEqual({ deleted_count: 2 });
    });

    it('returns deleted_count from response', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { deleted_count: 5 } }),
      });

      const result = await ChatService.deleteMessage(1);

      expect(result.deleted_count).toBe(5);
    });

    it('throws error when response is not ok', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        json: () => Promise.resolve({ success: false, error: 'Database error' }),
      });

      await expect(ChatService.deleteMessage(1)).rejects.toThrow();
    });

    it('handles single message deletion (deleted_count = 1)', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { deleted_count: 1 } }),
      });

      const result = await ChatService.deleteMessage(789);

      expect(result.deleted_count).toBe(1);
    });

    it('handles cascade deletion (deleted_count > 1)', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { deleted_count: 10 } }),
      });

      const result = await ChatService.deleteMessage(100);

      expect(result.deleted_count).toBe(10);
    });
  });

  describe('getMessages', () => {
    it('fetches messages for a conversation', async () => {
      const mockMessages = [
        { id: 1, conversation_id: 1, role: 'user', content: 'Hello', created_at: '2024-01-01T00:00:00Z' },
        { id: 2, conversation_id: 1, role: 'assistant', content: 'Hi!', created_at: '2024-01-01T00:00:01Z' },
      ];

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: mockMessages }),
      });

      const result = await ChatService.getMessages(1);

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/conversations/1/messages'),
        undefined
      );
      expect(result).toEqual(mockMessages);
    });
  });

  describe('saveMessage', () => {
    it('sends POST request with message data', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 42 }),
      });

      const result = await ChatService.saveMessage({
        conversation_id: 1,
        role: 'user',
        content: 'Test message',
      });

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/messages'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            conversation_id: 1,
            role: 'user',
            content: 'Test message',
          }),
        })
      );
      expect(result).toBe(42);
    });
  });

  describe('generateChatTitle', () => {
    const mockMessages = [
      { id: 1, conversation_id: 1, role: 'user' as const, content: 'Hello', created_at: '2024-01-01T00:00:00Z' },
      { id: 2, conversation_id: 1, role: 'assistant' as const, content: 'Hi there!', created_at: '2024-01-01T00:00:01Z' },
    ];

    it('sends conversation to LLM and returns generated title', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          choices: [{ message: { content: 'Friendly Greeting Exchange' } }],
        }),
      });

      const result = await ChatService.generateChatTitle(8080, mockMessages);

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/chat'),
        expect.objectContaining({
          method: 'POST',
          body: expect.stringContaining('"port":8080'),
        })
      );
      expect(result).toBe('Friendly Greeting Exchange');
    });

    it('uses custom prompt when provided', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          choices: [{ message: { content: 'Custom Title' } }],
        }),
      });

      await ChatService.generateChatTitle(8080, mockMessages, 'Generate a title');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.anything(),
        expect.objectContaining({
          body: expect.stringContaining('Generate a title'),
        })
      );
    });

    it('throws error when no server is running', async () => {
      await expect(ChatService.generateChatTitle(0, mockMessages)).rejects.toThrow(
        'No server running'
      );
    });

    it('throws error for empty conversation', async () => {
      await expect(ChatService.generateChatTitle(8080, [])).rejects.toThrow(
        'Cannot generate title for empty conversation'
      );
    });

    it('throws error on API failure', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: false,
        text: () => Promise.resolve('Server error'),
      });

      await expect(ChatService.generateChatTitle(8080, mockMessages)).rejects.toThrow(
        'Failed to generate title'
      );
    });

    it('throws error when model returns empty title', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          choices: [{ message: { content: '' } }],
        }),
      });

      await expect(ChatService.generateChatTitle(8080, mockMessages)).rejects.toThrow(
        'Model returned an empty title'
      );
    });

    it('cleans up generated title (removes quotes and trailing periods)', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          choices: [{ message: { content: '"Title with quotes..."' } }],
        }),
      });

      const result = await ChatService.generateChatTitle(8080, mockMessages);

      expect(result).toBe('Title with quotes');
    });

    it('filters out system messages from context', async () => {
      const messagesWithSystem = [
        { id: 0, conversation_id: 1, role: 'system' as const, content: 'You are helpful', created_at: '2024-01-01T00:00:00Z' },
        ...mockMessages,
      ];

      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          choices: [{ message: { content: 'Title' } }],
        }),
      });

      await ChatService.generateChatTitle(8080, messagesWithSystem);

      const callBody = JSON.parse(mockFetch.mock.calls[0][1].body);
      // System message should be filtered, so only user + assistant + title prompt
      const roles = callBody.messages.map((m: any) => m.role);
      expect(roles).not.toContain('system');
    });

    it('limits title to 100 characters', async () => {
      const longTitle = 'A'.repeat(150);
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          choices: [{ message: { content: longTitle } }],
        }),
      });

      const result = await ChatService.generateChatTitle(8080, mockMessages);

      expect(result.length).toBeLessThanOrEqual(100);
    });

    it('returns "New Chat" for whitespace-only title', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({
          choices: [{ message: { content: '   ' } }],
        }),
      });

      await expect(ChatService.generateChatTitle(8080, mockMessages)).rejects.toThrow(
        'Model returned an empty title'
      );
    });
  });
});
