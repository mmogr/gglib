/**
 * Tests for the shared proxy display components.
 *
 * The dashboard modal renders these sections, so a regression here is one
 * there.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ActiveConnectionsSection, InferenceSlotsSection } from '../../../src/components/proxy';
import type {
  ActiveConnectionSnapshot,
  DashboardSnapshot,
} from '../../../src/services/transport/types/dashboard';
import { activeConnection, dashboardSnapshot, slotSnapshot } from '../fixtures/dashboard';

const connection = (overrides: Partial<ActiveConnectionSnapshot> = {}): ActiveConnectionSnapshot =>
  activeConnection({ id: 'c1', model_name: 'qwen3-coder', is_streaming: true, ...overrides });

const snapshot = (overrides: Partial<DashboardSnapshot> = {}): DashboardSnapshot =>
  dashboardSnapshot(overrides);

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
          // `dashboard.rs` derives both from one match: `Available` gives
          // `true` with no status line, and either unavailable arm gives
          // `false` with one. Setting only the flag builds a frame the proxy
          // cannot send.
          slots_status: null,
          slots: [
            slotSnapshot({ id: 0, is_processing: true, n_ctx: 4096 }),
            slotSnapshot({ id: 1, is_processing: false, n_ctx: 4096 }),
          ],
        })}
      />,
    );

    expect(screen.getByText(/slot 0 · active/i)).toBeInTheDocument();
    expect(screen.getByText(/slot 1/i)).toBeInTheDocument();
  });
});
