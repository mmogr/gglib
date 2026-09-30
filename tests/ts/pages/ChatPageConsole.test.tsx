/**
 * The Console view inside the notebook's frame.
 *
 * It used to replace the whole page with the pre-notebook two-panel layout
 * and a second tab bar of its own, so switching views lost the conversation
 * rail and the notebook's head. Now the rail and the head stay, the head's
 * margin keeps the view switcher, and only the body below changes.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { chatTransport, conversation, wrapper, type ChatFixture } from './chatPageHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});
// The log is the daemon's; one line stands in for it, and no stream opens.
vi.mock('../../../src/hooks/useServerLogs', () => ({
  useServerLogs: () => ({
    logs: [{ timestamp: 1, line: 'main: server is listening on 127.0.0.1:4321', port: 4321 }],
    clearLogs: () => {},
    isAutoScroll: true,
    setIsAutoScroll: () => {},
    copyAllLogs: () => {},
  }),
}));

import ChatPage from '../../../src/pages/ChatPage';

let fixture: ChatFixture;

beforeEach(() => {
  window.localStorage.clear();
  fixture = {
    conversations: [conversation(1, 'launchd KeepAlive')],
    rows: { 1: [{ id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' }] },
    runs: [],
    frames: {},
  };
  transport.current = chatTransport(fixture);
});

describe('ChatPage, console view', () => {
  it('keeps the rail and the head margin, and shows the server and its log', async () => {
    const user = userEvent.setup();
    render(<ChatPage modelName="qwen3" modelId={7} serverPort={4321} onClose={async () => {}} />, { wrapper });
    // Once the saved turn is drawn, the thread has stopped remounting.
    await screen.findByText('What does KeepAlive do?');
    expect(screen.getByLabelText('Server output').closest('.hidden')).not.toBeNull();

    await user.click(screen.getByRole('tab', { name: /console/i }));

    // The rail and the head, with its view switcher and Close, are still up.
    // Hidden by class as well as attribute, so both are checked.
    const shown = (el: HTMLElement) => el.closest('.hidden,[hidden]') === null;
    expect(shown(screen.getByRole('button', { name: /^Conversations/ }))).toBe(true);
    const head = screen.getByRole('heading', { name: 'launchd KeepAlive' }).closest('.grid') as HTMLElement;
    expect(shown(head)).toBe(true);
    expect(within(head).getByRole('tab', { name: /console/i })).toHaveAttribute('aria-selected', 'true');
    expect(within(head).getByRole('button', { name: 'Close' })).toBeVisible();
    // One view switcher: the console no longer carries its own.
    expect(screen.getAllByRole('tablist', { name: 'Chat views' })).toHaveLength(1);

    // The body is the server and its log; the thread is hidden, not gone.
    const log = screen.getByLabelText('Server output');
    expect(shown(log)).toBe(true);
    expect(log).toHaveTextContent('server is listening');
    expect(screen.getByRole('button', { name: 'Stop Server' })).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'Message' })).not.toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Message', hidden: true })).toBeInTheDocument();
  });
});
