import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import HitlApprovalModal from '../../../src/pages/Council/components/HitlApprovalModal';
import type { RunCostEstimate } from '../../../src/contexts/CouncilContext';

const baseProps = {
  open: true,
  kind: { kind: 'plan' } as const,
  graph: null,
  submitting: false,
  costEstimate: null,
  onApprove: vi.fn(),
  onReject: vi.fn(),
};

describe('HitlApprovalModal — cost warning banner', () => {
  it('does not render the banner when costEstimate is null', () => {
    render(<HitlApprovalModal {...baseProps} costEstimate={null} />);
    expect(screen.queryByTestId('cost-warning-banner')).not.toBeInTheDocument();
  });

  it('does not render the banner when estimate is below thresholds', () => {
    const est: RunCostEstimate = {
      nodeCount: 2,
      estTokens: 4_000,
      estWallSeconds: 80, // > 60 → should show... let's use 50 to be below
    };
    // Correct: wall 50 ≤ 60 AND nodeCount 2 ≤ 25*0.8 = 20 → no banner
    const safeEst: RunCostEstimate = { nodeCount: 2, estTokens: 4_000, estWallSeconds: 50 };
    render(<HitlApprovalModal {...baseProps} costEstimate={safeEst} />);
    expect(screen.queryByTestId('cost-warning-banner')).not.toBeInTheDocument();
  });

  it('renders the banner when est_wall_seconds exceeds 60', () => {
    const est: RunCostEstimate = {
      nodeCount: 3,
      estTokens: 6_000,
      estWallSeconds: 120,
    };
    render(<HitlApprovalModal {...baseProps} costEstimate={est} />);
    expect(screen.getByTestId('cost-warning-banner')).toBeInTheDocument();
  });

  it('renders the banner when node_count exceeds 80% of budgetUpper', () => {
    // budgetUpper defaults to 25; 80% = 20; nodeCount 21 → show
    const est: RunCostEstimate = {
      nodeCount: 21,
      estTokens: 42_000,
      estWallSeconds: 55, // < 60, but nodeCount > 20 → show
    };
    render(<HitlApprovalModal {...baseProps} costEstimate={est} />);
    expect(screen.getByTestId('cost-warning-banner')).toBeInTheDocument();
  });

  it('renders the banner for a massive estimate (1000 nodes)', () => {
    const est: RunCostEstimate = {
      nodeCount: 1_000,
      estTokens: 2_000_000,
      estWallSeconds: 40_000,
    };
    render(<HitlApprovalModal {...baseProps} costEstimate={est} />);
    expect(screen.getByTestId('cost-warning-banner')).toBeInTheDocument();
  });

  it('Approve button is always enabled regardless of cost estimate', () => {
    const est: RunCostEstimate = {
      nodeCount: 1_000,
      estTokens: 2_000_000,
      estWallSeconds: 40_000,
    };
    render(<HitlApprovalModal {...baseProps} costEstimate={est} />);
    // There may be multiple Approve-labelled buttons; all should be enabled.
    const approveBtns = screen.getAllByRole('button', { name: /approve/i });
    expect(approveBtns.length).toBeGreaterThan(0);
    approveBtns.forEach((btn) => expect(btn).not.toBeDisabled());
  });

  it('respects custom budgetUpper for the node threshold', () => {
    // budgetUpper=10; nodeCount=9; 9 > 10*0.8=8 → show banner
    const est: RunCostEstimate = {
      nodeCount: 9,
      estTokens: 18_000,
      estWallSeconds: 50, // < 60
    };
    render(<HitlApprovalModal {...baseProps} costEstimate={est} budgetUpper={10} />);
    expect(screen.getByTestId('cost-warning-banner')).toBeInTheDocument();
  });
});
