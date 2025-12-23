import { useEffect, useMemo, useRef, useState } from 'react';
import { AlertCircle, Loader2, Send } from 'lucide-react';
import { toast } from 'sonner';

import { api } from '../services/api';
import type { Model, ServerStatus } from '../types/api';
import type { ConversationId } from '../../services/transport/types/ids';
import type { ChatMessage } from '../../services/transport/types/chat';
import { Alert, AlertDescription } from './ui/alert';
import { Button } from './ui/button';
import { ScrollArea } from './ui/scroll-area';
import { Textarea } from './ui/textarea';
import { cn } from './ui/utils';

interface ChatInterfaceProps {
  model: Model;
  serverStatus?: ServerStatus;
  onServerChange: () => void | Promise<void>;
}

export function ChatInterface({ model, serverStatus, onServerChange }: ChatInterfaceProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [conversationId, setConversationId] = useState<ConversationId | null>(null);
  const [sending, setSending] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  const modelId = model.id ?? null;
  const serverPort = serverStatus?.port;
  const serverState = serverStatus?.status ?? 'stopped';
  const isRunning = serverState === 'running' && typeof serverPort === 'number';

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, sending]);

  useEffect(() => {
    void loadConversationAndMessages();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [modelId]);

  const loadConversationAndMessages = async () => {
    if (!modelId) return;
    try {
      const conversations = await api.listConversations();
      const existing = conversations.find((c) => c.model_id === modelId);

      const id = existing
        ? existing.id
        : await api.createConversation({ title: 'New Chat', modelId, systemPrompt: null });

      setConversationId(id);
      const msgs = await api.getMessages(id);
      setMessages(msgs);
    } catch (error) {
      console.error('Failed to load conversation:', error);
    }
  };

  const llamaMessages = useMemo(
    () =>
      messages.map((m) => ({
        role: m.role,
        content: m.content,
      })),
    [messages]
  );

  const handleStartServer = async () => {
    if (!modelId) return;
    try {
      await api.startServer(modelId, model.context_length ?? undefined);
      toast.success('Server starting');
      await onServerChange();
    } catch (error) {
      toast.error('Failed to start server');
      console.error(error);
    }
  };

  const handleSend = async () => {
    if (!input.trim() || !conversationId || sending) return;
    if (!isRunning) return;

    const userMessage = input.trim();
    setInput('');
    setSending(true);

    try {
      const userId = await api.saveMessage(conversationId, 'user', userMessage);
      const userMsg: ChatMessage = {
        id: userId,
        conversation_id: conversationId,
        role: 'user',
        content: userMessage,
        created_at: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, userMsg]);

      const response = await fetch(`http://127.0.0.1:${serverPort}/v1/chat/completions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          messages: [...llamaMessages, { role: 'user', content: userMessage }],
          temperature: 0.7,
        }),
      });

      if (!response.ok) {
        throw new Error(`Chat failed: ${response.statusText}`);
      }

      const data = await response.json();
      const assistantContent: string = data?.choices?.[0]?.message?.content ?? '';

      const assistantId = await api.saveMessage(conversationId, 'assistant', assistantContent);
      const assistantMsg: ChatMessage = {
        id: assistantId,
        conversation_id: conversationId,
        role: 'assistant',
        content: assistantContent,
        created_at: new Date().toISOString(),
      };
      setMessages((prev) => [...prev, assistantMsg]);
    } catch (error) {
      toast.error('Failed to send message');
      console.error(error);
    } finally {
      setSending(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  };

  return (
    <div className="flex flex-col h-full">
      <div className="px-6 py-4 border-b border-border bg-card">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="font-semibold text-lg">{model.name}</h2>
            <p className="text-sm text-muted-foreground">
              {model.param_count_b ? `${model.param_count_b}B • ` : ''}
              {model.quantization ?? 'Unknown quantization'}
            </p>
          </div>
          {serverStatus && (
            <div
              className={cn(
                'px-3 py-1 rounded-full text-sm font-medium',
                isRunning ? 'bg-green-500/10 text-green-600 dark:text-green-400' : 'bg-muted text-muted-foreground'
              )}
            >
              {serverState}
              {serverPort ? ` • Port ${serverPort}` : ''}
            </div>
          )}
        </div>
      </div>

      <ScrollArea className="flex-1 p-6" ref={scrollRef}>
        {!isRunning ? (
          <Alert>
            <AlertCircle className="size-4" />
            <AlertDescription>
              Server is not running. Start the server to begin chatting.
              <Button variant="outline" size="sm" className="ml-4" onClick={() => void handleStartServer()}>
                Start Server
              </Button>
            </AlertDescription>
          </Alert>
        ) : messages.length === 0 ? (
          <div className="text-center text-muted-foreground py-12">
            <p>No messages yet. Start a conversation!</p>
          </div>
        ) : (
          <div className="space-y-6 max-w-3xl mx-auto">
            {messages.map((message) => (
              <div
                key={message.id}
                className={cn('flex gap-4', message.role === 'user' ? 'justify-end' : 'justify-start')}
              >
                <div
                  className={cn(
                    'rounded-lg px-4 py-3 max-w-[80%]',
                    message.role === 'user' ? 'bg-primary text-primary-foreground' : 'bg-muted'
                  )}
                >
                  <div className="prose prose-sm dark:prose-invert max-w-none whitespace-pre-wrap">
                    {message.content}
                  </div>
                  <div className="text-xs opacity-60 mt-2">
                    {new Date(message.created_at).toLocaleTimeString()}
                  </div>
                </div>
              </div>
            ))}
            {sending && (
              <div className="flex gap-4 justify-start">
                <div className="rounded-lg px-4 py-3 bg-muted">
                  <Loader2 className="size-4 animate-spin" />
                </div>
              </div>
            )}
          </div>
        )}
      </ScrollArea>

      <div className="p-4 border-t border-border bg-card">
        <div className="max-w-3xl mx-auto flex gap-2">
          <Textarea
            value={input}
            onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={isRunning ? 'Type your message... (Enter to send, Shift+Enter for new line)' : 'Start the server to begin chatting'}
            disabled={!isRunning || sending}
            className="min-h-[60px] max-h-[200px] resize-none"
          />
          <Button
            onClick={() => void handleSend()}
            disabled={!isRunning || !input.trim() || sending}
            size="icon"
            className="shrink-0 size-[60px]"
          >
            {sending ? <Loader2 className="size-5 animate-spin" /> : <Send className="size-5" />}
          </Button>
        </div>
      </div>
    </div>
  );
}
