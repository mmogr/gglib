/**
 * Tests for the shared proxy display components.
 *
 * These render on three surfaces (the header dropdown, the dashboard modal
 * and the tray popover), so a regression here is a regression everywhere.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import {
  ActiveConnectionsSection,
  EndpointCopyBar,
  InferenceSlotsSection,
  ProxyStatusPill,
  ProxyToggleButton,
  proxyEndpointUrl,
} from '../../../src/components/proxy';
import type {
  ActiveConnectionSnapshot,
  DashboardSnapshot,
} from '../../../src/services/transport/types/dashboard';

function connection(overrides: Partial<ActiveConnectionSnapshot> = {}): ActiveConnectionSnapshot {
  return {
    id: 'c1',
    model_name: 'qwen3-coder',
    started_at_secs: 0,
    is_streaming: true,
    phase: 'generating',
    ...overrides,
  };
}

function snapshot(overrides: Partial<DashboardSnapshot> = {}): DashboardSnapshot {
  return {
    active_connections: [],
    slots: [],
    slots_available: false,
    slots_status: 'Slot metrics unavailable.',
    total_requests: 0,
    ...overrides,
  } as DashboardSnapshot;
}

describe('ProxyStatusPill', () => {
  it('reads Running when the proxy is up', () => {
    render(<ProxyStatusPill running />);
    expect(screen.getByText('Running')).toBeInTheDocument();
  });

  /**
   * A stopped proxy is idle, not broken. Danger red is reserved for real
   * failures, so the stopped pill must not carry it.
   */
  it('styles a stopped proxy as idle rather than as an error', () => {
    render(<ProxyStatusPill running={false} />);
    const pill = screen.getByText('Stopped');
    expect(pill.className).toContain('text-offline');
    expect(pill.className).not.toContain('danger');
  });
});

describe('proxyEndpointUrl', () => {
  it('builds the /v1 base URL clients are pointed at', () => {
    expect(proxyEndpointUrl('127.0.0.1', 8080)).toBe('http://127.0.0.1:8080/v1');
  });
});

describe('EndpointCopyBar', () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it('shows the endpoint URL', () => {
    render(<EndpointCopyBar host="127.0.0.1" port={8080} />);
    expect(screen.getByText('http://127.0.0.1:8080/v1')).toBeInTheDocument();
  });

  it('copies the URL and reports it back to the caller', async () => {
    const onCopied = vi.fn();
    render(<EndpointCopyBar host="127.0.0.1" port={9000} onCopied={onCopied} />);

    await userEvent.click(screen.getByTitle('Copy URL'));

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('http://127.0.0.1:9000/v1');
    expect(onCopied).toHaveBeenCalledWith('http://127.0.0.1:9000/v1');
  });
});

describe('ProxyToggleButton', () => {
  it('offers to start when stopped, and calls onStart', async () => {
    const onStart = vi.fn();
    const onStop = vi.fn();
    render(
      <ProxyToggleButton running={false} pending={false} onStart={onStart} onStop={onStop} />,
    );

    await userEvent.click(screen.getByRole('button', { name: /start proxy/i }));

    expect(onStart).toHaveBeenCalledOnce();
    expect(onStop).not.toHaveBeenCalled();
  });

  it('offers to stop when running, and calls onStop', async () => {
    const onStart = vi.fn();
    const onStop = vi.fn();
    render(<ProxyToggleButton running pending={false} onStart={onStart} onStop={onStop} />);

    await userEvent.click(screen.getByRole('button', { name: /stop proxy/i }));

    expect(onStop).toHaveBeenCalledOnce();
    expect(onStart).not.toHaveBeenCalled();
  });

  it('disables itself and shows progress while pending', () => {
    render(<ProxyToggleButton running pending onStart={vi.fn()} onStop={vi.fn()} />);
    const button = screen.getByRole('button', { name: /stopping/i });
    expect(button).toBeDisabled();
  });
});

describe('ActiveConnectionsSection', () => {
  it('says so when nothing is in flight', () => {
    render(<ActiveConnectionsSection snapshot={snapshot()} />);
    expect(screen.getByText(/no active connections/i)).toBeInTheDocument();
  });

  it('counts and lists in-flight requests', () => {
    render(
      <ActiveConnectionsSection
        snapshot={snapshot({ active_connections: [connection(), connection({ id: 'c2' })] })}
      />,
    );

    expect(screen.getByText(/active connections \(2\)/i)).toBeInTheDocument();
    expect(screen.getAllByText('qwen3-coder')).toHaveLength(2);
  });

  /**
   * There is no denominator to measure generation against, so the prompt bar
   * belongs to the prompt phase only.
   */
  it('shows prompt progress only while the prompt is being processed', () => {
    const { rerender, container } = render(
      <ActiveConnectionsSection
        snapshot={snapshot({ active_connections: [connection({ phase: 'generating' })] })}
      />,
    );
    expect(screen.getByText('Generating')).toBeInTheDocument();
    const generatingBars = container.querySelectorAll('progress, [role="progressbar"]');

    rerender(
      <ActiveConnectionsSection
        snapshot={snapshot({
          active_connections: [
            connection({ phase: 'processing_prompt', prompt_processed: 5, prompt_total: 10 }),
          ],
        })}
      />,
    );
    expect(screen.getByText('Processing prompt')).toBeInTheDocument();
    expect(
      container.querySelectorAll('progress, [role="progressbar"]').length,
    ).toBeGreaterThanOrEqual(generatingBars.length);
  });
});

describe('InferenceSlotsSection', () => {
  /**
   * When llama.cpp is not reporting slots the snapshot carries the reason;
   * surfacing it beats an empty panel that looks like a bug.
   */
  it('surfaces the snapshot status when slots are unavailable', () => {
    render(
      <InferenceSlotsSection
        snapshot={snapshot({ slots_available: false, slots_status: 'No model loaded.' })}
      />,
    );
    expect(screen.getByText('No model loaded.')).toBeInTheDocument();
  });

  it('renders a card per slot when they are available', () => {
    render(
      <InferenceSlotsSection
        snapshot={snapshot({
          slots_available: true,
          slots: [
            { id: 0, is_processing: true, n_ctx: 4096 },
            { id: 1, is_processing: false, n_ctx: 4096 },
          ],
        })}
      />,
    );

    expect(screen.getByText(/slot 0 · active/i)).toBeInTheDocument();
    expect(screen.getByText(/slot 1/i)).toBeInTheDocument();
  });
});
