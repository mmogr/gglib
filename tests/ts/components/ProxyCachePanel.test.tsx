import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ProxyCachePanel } from '../../../src/places/diagnostics/live/ProxyCachePanel';
import type { CacheStatus, CacheUsage } from '../../../src/services/transport/types/dashboard';

const emptyUsage: CacheUsage = {
  reporting_requests: 0,
  unreported_requests: 0,
  prompt_tokens: 0,
  cached_tokens: 0,
  last_prompt_tokens: null,
  last_cached_tokens: null,
  reuse_percent: null,
  summary: null,
  last_reuse_percent: null,
};

function status(overrides: Partial<CacheStatus> = {}): CacheStatus {
  return {
    disk_enabled: true,
    disk_suppressed_for_model: false,
    ram_budget_mb: 70008,
    ram_state: 'healthy',
    needs_attention: false,
    warnings: [],
    usage: emptyUsage,
    ...overrides,
  };
}

describe('ProxyCachePanel', () => {
  it('reports that no model has resolved yet when cache is absent', () => {
    render(<ProxyCachePanel cache={null} />);
    expect(screen.getByText(/no model resolved yet/i)).toBeInTheDocument();
  });

  it('shows measured reuse totals with thousands separators', () => {
    render(
      <ProxyCachePanel
        cache={status({
          usage: {
            ...emptyUsage,
            reporting_requests: 3,
            prompt_tokens: 30342,
            cached_tokens: 29450,
            last_prompt_tokens: 10000,
            last_cached_tokens: 9500,
            reuse_percent: 97,
            summary: 'Prompt cache 97% reused',
            last_reuse_percent: 95,
          },
        })}
      />,
    );
    expect(screen.getByText('29,450 tokens')).toBeInTheDocument();
    expect(screen.getByText('30,342 tokens')).toBeInTheDocument();
    expect(screen.getByText('9,500 of 10,000 tokens from cache')).toBeInTheDocument();
  });

  // "Nothing measured yet" and "measured, and it was zero" are different
  // facts; the backend keeps them apart, so the panel must too.
  it('distinguishes no activity yet from a measured zero', () => {
    const { rerender } = render(<ProxyCachePanel cache={status()} />);
    expect(screen.getByText(/no cache activity recorded yet/i)).toBeInTheDocument();

    rerender(
      <ProxyCachePanel
        cache={status({
          usage: {
            ...emptyUsage,
            reporting_requests: 1,
            prompt_tokens: 5000,
            cached_tokens: 0,
            last_prompt_tokens: 5000,
            last_cached_tokens: 0,
          },
        })}
      />,
    );
    expect(screen.queryByText(/no cache activity recorded yet/i)).not.toBeInTheDocument();
    expect(screen.getByText('0 tokens')).toBeInTheDocument();
  });

  it('renders every backend warning verbatim', () => {
    render(
      <ProxyCachePanel
        cache={status({
          needs_attention: true,
          warnings: ['First warning line.', 'Second warning line.'],
        })}
      />,
    );
    expect(screen.getByText('First warning line.')).toBeInTheDocument();
    expect(screen.getByText('Second warning line.')).toBeInTheDocument();
  });

  // A permanent "0" row would be noise on any current llama.cpp, which always
  // reports the field.
  it('hides the unreported-requests row unless some requests lacked data', () => {
    const { rerender } = render(
      <ProxyCachePanel cache={status({ usage: { ...emptyUsage, reporting_requests: 1 } })} />,
    );
    expect(screen.queryByText(/requests without cache data/i)).not.toBeInTheDocument();

    rerender(
      <ProxyCachePanel
        cache={status({
          usage: { ...emptyUsage, reporting_requests: 1, unreported_requests: 2 },
        })}
      />,
    );
    expect(screen.getByText(/requests without cache data/i)).toBeInTheDocument();
  });

  it('describes the disk layer as off for this model when suppressed', () => {
    render(<ProxyCachePanel cache={status({ disk_suppressed_for_model: true })} />);
    expect(screen.getByText(/off for this model/i)).toBeInTheDocument();
  });

  it('omits a RAM budget figure when llama-server’s own default applies', () => {
    render(<ProxyCachePanel cache={status({ ram_state: 'llama_default', ram_budget_mb: null })} />);
    expect(screen.queryByText(/RAM budget/i)).not.toBeInTheDocument();
  });

  it('explains a budget that the machine could not afford', () => {
    render(
      <ProxyCachePanel
        cache={status({ ram_state: 'disabled_insufficient_ram', ram_budget_mb: 0 })}
      />,
    );
    expect(screen.getByText(/not enough memory/i)).toBeInTheDocument();
  });

  // The percents are the proxy's: the panel shows what it was sent and
  // divides nothing, so a frame without them shows none.
  it('shows the proxy\'s reuse percents as it sent them, and none it did not send', () => {
    const measured = { ...emptyUsage, reporting_requests: 2, prompt_tokens: 10000, cached_tokens: 6600, last_prompt_tokens: 4000, last_cached_tokens: 2400 };
    const { rerender } = render(
      <ProxyCachePanel cache={status({ usage: { ...measured, reuse_percent: 66, summary: 'Prompt cache 66% reused', last_reuse_percent: 60 } })} />,
    );
    expect(screen.getByText('Cache hit rate').parentElement).toHaveTextContent('Cache hit rate66 %');
    expect(screen.getByText('Last request reuse').parentElement).toHaveTextContent('Last request reuse60 %');

    rerender(<ProxyCachePanel cache={status({ usage: measured })} />);
    expect(screen.queryByText('Cache hit rate')).not.toBeInTheDocument();
    expect(screen.queryByText('Last request reuse')).not.toBeInTheDocument();
  });
});
