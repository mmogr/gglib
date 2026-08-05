/**
 * The panel that explains slowness.
 *
 * Two things matter here and nothing else does. First, that a refusal to
 * co-load a second model is always *explained* — an idle second slot on a card
 * with free VRAM is the single most confusing thing this dashboard can show,
 * and the backend sends prose specifically so the UI never has to guess.
 * Second, that queue depth and swap count are shown together: either alone
 * misleads, since a backlog with no swaps behind it is the batching working.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import ProxyAdmissionPanel from '../../../src/components/ProxyAdmissionPanel';
import type {
  AdmissionSnapshot,
  ResidentSlotSnapshot,
} from '../../../src/services/transport/types/admission';

function slot(overrides: Partial<ResidentSlotSnapshot> = {}): ResidentSlotSnapshot {
  return {
    slot: 0,
    model_name: 'qwen-coder',
    model_id: 1,
    port: 8080,
    inflight: 0,
    is_primary: true,
    resident_for_secs: 12,
    ...overrides,
  };
}

function admission(overrides: Partial<AdmissionSnapshot> = {}): AdmissionSnapshot {
  return {
    slots: [],
    queued: [],
    total_queued: 0,
    total_swaps: 0,
    secondary_slot: { state: 'available', detail: 'No second model has been requested yet.' },
    ...overrides,
  };
}

describe('ProxyAdmissionPanel', () => {
  it('says so plainly when the proxy is not reporting admission at all', () => {
    render(<ProxyAdmissionPanel admission={null} />);
    expect(screen.getByText(/not reporting/i)).toBeInTheDocument();
  });

  it('reports an empty resident set rather than rendering nothing', () => {
    render(<ProxyAdmissionPanel admission={admission()} />);
    expect(screen.getByText(/no model is loaded yet/i)).toBeInTheDocument();
  });

  it('names each resident model and whether it is primary', () => {
    render(
      <ProxyAdmissionPanel
        admission={admission({
          slots: [
            slot(),
            slot({ slot: 1, model_name: 'nomic-embed', is_primary: false, model_id: 2 }),
          ],
        })}
      />,
    );

    expect(screen.getByText('qwen-coder')).toBeInTheDocument();
    expect(screen.getByText('nomic-embed')).toBeInTheDocument();
    expect(screen.getByText('primary')).toBeInTheDocument();
    expect(screen.getByText('secondary')).toBeInTheDocument();
  });

  it('distinguishes a slot that is serving from one that is idle', () => {
    render(<ProxyAdmissionPanel admission={admission({ slots: [slot({ inflight: 3 })] })} />);
    expect(screen.getByText(/3 in flight/)).toBeInTheDocument();

    render(<ProxyAdmissionPanel admission={admission({ slots: [slot({ inflight: 0 })] })} />);
    expect(screen.getAllByText(/idle/).length).toBeGreaterThan(0);
  });

  /// The reason the backend carries prose at all.
  it('renders the second slot explanation verbatim, whatever it says', () => {
    const detail =
      'Not enough free VRAM to keep a second model loaded: it needs about 307 MiB, and only 200 MiB is free.';
    render(
      <ProxyAdmissionPanel
        admission={admission({ secondary_slot: { state: 'no_headroom', detail } })}
      />,
    );

    expect(screen.getByText(detail)).toBeInTheDocument();
  });

  it('explains an unreadable VRAM budget rather than showing an empty slot', () => {
    render(
      <ProxyAdmissionPanel
        admission={admission({
          secondary_slot: {
            state: 'unknown_budget',
            detail: 'gglib cannot read this machine’s free VRAM.',
          },
        })}
      />,
    );

    expect(screen.getByText(/cannot read this machine/i)).toBeInTheDocument();
  });

  it('shows queue depth and the oldest wait for each waiting model', () => {
    render(
      <ProxyAdmissionPanel
        admission={admission({
          queued: [{ model_name: 'nomic-embed', waiting: 4, oldest_wait_ms: 2400 }],
        })}
      />,
    );

    expect(screen.getByText('nomic-embed')).toBeInTheDocument();
    expect(screen.getByText(/4 waiting/)).toBeInTheDocument();
    expect(screen.getByText(/oldest 2s/)).toBeInTheDocument();
  });

  it('renders a long wait in minutes rather than a large second count', () => {
    render(
      <ProxyAdmissionPanel
        admission={admission({
          queued: [{ model_name: 'nomic-embed', waiting: 1, oldest_wait_ms: 95_000 }],
        })}
      />,
    );

    expect(screen.getByText(/oldest 1m 35s/)).toBeInTheDocument();
  });

  /// Neither figure means anything alone — see the module docs.
  it('always shows admitted requests alongside swaps', () => {
    render(
      <ProxyAdmissionPanel
        admission={admission({ total_queued: 1234, total_swaps: 2 })}
      />,
    );

    expect(screen.getByText('Requests admitted')).toBeInTheDocument();
    expect(screen.getByText('1,234')).toBeInTheDocument();
    expect(screen.getByText('Model swaps')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });
});
