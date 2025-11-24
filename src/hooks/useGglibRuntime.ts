import { useLocalRuntime, type ChatModelAdapter } from '@assistant-ui/react';
import { TauriService } from '../services/tauri';
import { getApiBase } from '../utils/apiBase';

export interface ServerInfo {
  model_id: number;
  model_name: string;
  port: number;
  status: string;
}

export interface GglibRuntimeOptions {
  selectedServerPort?: number;
  onError?: (error: Error) => void;
}

/**
 * Custom runtime hook that bridges assistant-ui with gglib's served models.
 * 
 * This hook creates a runtime that:
 * - Connects directly to running llama-server instances via /api/chat
 * - Forwards requests to the selected server port
 * - Supports OpenAI-compatible responses
 * - Works in both Tauri desktop and web modes
 * 
 * @param options - Configuration options for the runtime
 * @returns The assistant-ui runtime instance
 */
export function useGglibRuntime(options: GglibRuntimeOptions = {}) {
  const { selectedServerPort, onError } = options;

  // Create adapter for gglib direct server API
  const gglibAdapter: ChatModelAdapter = {
    async run({ messages, abortSignal }) {
      if (!selectedServerPort) {
        const error = new Error('No server selected. Please serve a model first.');
        onError?.(error);
        throw error;
      }

      console.log('🚀 Sending chat request to port:', selectedServerPort);
      console.log('📝 Messages:', messages);

      const requestBody = {
        port: selectedServerPort,
        model: 'default', // llama-server ignores this but we include for consistency
        messages: messages.map((m) => ({
          role: m.role,
          content: m.content
            .filter((c) => c.type === 'text')
            .map((c) => c.text)
            .join('\n'),
        })),
        stream: false,
      };

      console.log('📤 Request body:', JSON.stringify(requestBody, null, 2));

      const apiBase = await getApiBase();
      const response = await fetch(`${apiBase}/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(requestBody),
        signal: abortSignal,
      });

      console.log('📥 Response status:', response.status);

      if (!response.ok) {
        const errorText = await response.text();
        console.error('❌ API error:', errorText);
        const error = new Error(`API error (${response.status}): ${errorText}`);
        onError?.(error);
        throw error;
      }

      const data = await response.json();
      console.log('✅ Response data:', data);

      const messageContent = data.choices?.[0]?.message?.content || '';
      console.log('💬 Extracted content:', messageContent);

      return {
        content: [{ type: 'text', text: messageContent }],
      };
    },
  };

  const runtime = useLocalRuntime(gglibAdapter);
  return runtime;
}

/**
 * Fetch available servers (served models) from gglib
 * 
 * @returns Promise resolving to array of server info objects
 */
export async function fetchAvailableServers(): Promise<ServerInfo[]> {
  try {
    return await TauriService.listServers();
  } catch (error) {
    console.error('Error fetching servers:', error);
    return [];
  }
}
