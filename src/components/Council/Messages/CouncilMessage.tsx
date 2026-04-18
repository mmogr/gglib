/**
 * Renders a single agent's turn in the council deliberation.
 *
 * Ambient tinting via `--agent-color` CSS custom property. Subtle left
 * border + faint background via `color-mix()`. Streaming-aware: shows
 * either the completed contribution text or live `activeText`.
 *
 * @module components/Council/Messages/CouncilMessage
 */

import type { FC } from 'react';
import { Wrench, CheckCircle, XCircle } from 'lucide-react';
import type { AgentContribution, AgentToolCall } from '../../../types/council';
import { contentiousnessColor } from '../../../types/council';
import { cn } from '../../../utils/cn';
import { Icon } from '../../ui/Icon';
import MarkdownMessageContent from '../../ChatMessagesPanel/components/MarkdownMessageContent';

// ─── Tool call display (lightweight, no registry dependency) ────────────────

const ToolCallBadge: FC<{ call: AgentToolCall }> = ({ call }) => {
  const done = call.result !== undefined;
  const isError = call.result?.isError ?? false;

  return (
    <div className="flex items-center gap-xs text-xs text-text-secondary py-[2px]">
      <Icon
        icon={done ? (isError ? XCircle : CheckCircle) : Wrench}
        size={12}
        className={cn(
          done && !isError && 'text-success',
          done && isError && 'text-danger',
          !done && 'text-text-muted animate-pulse',
        )}
      />
      <span className="font-medium">{call.displayName}</span>
      {call.argsSummary && (
        <span className="text-text-muted truncate max-w-[180px]">{call.argsSummary}</span>
      )}
      {call.durationDisplay && (
        <span className="text-text-muted">({call.durationDisplay})</span>
      )}
    </div>
  );
};

// ─── Core claim pill ────────────────────────────────────────────────────────

const CoreClaimPill: FC<{ claim: string }> = ({ claim }) => (
  <div className="mt-sm pt-sm border-t border-[color-mix(in_srgb,var(--agent-color)_15%,var(--color-border))]">
    <span className="inline-flex items-center gap-xs text-xs px-sm py-[2px] rounded-full bg-[color-mix(in_srgb,var(--agent-color)_12%,var(--color-surface))] text-text-secondary">
      <span className="font-medium">Claim:</span>
      <span className="italic">{claim}</span>
    </span>
  </div>
);

// ─── Main message ───────────────────────────────────────────────────────────

export interface CouncilMessageProps {
  /** Completed contribution (for finished turns). */
  contribution?: AgentContribution;
  /** Live-streaming state (for the active turn). */
  streaming?: {
    agentId: string;
    agentName: string;
    color: string;
    contentiousness: number;
    text: string;
    reasoning: string;
    toolCalls: AgentToolCall[];
  };
}

export const CouncilMessage: FC<CouncilMessageProps> = ({ contribution, streaming }) => {
  const source = contribution ?? streaming;
  if (!source) return null;

  const color = contentiousnessColor(source.contentiousness);
  const text = contribution?.content ?? streaming?.text ?? '';
  const reasoning = streaming?.reasoning ?? '';
  const toolCalls = streaming?.toolCalls ?? [];
  const coreClaim = contribution?.coreClaim;
  const isStreaming = !!streaming;

  return (
    <div
      className={cn(
        'rounded-base border-l-[3px] p-md transition-colors duration-200',
        'bg-[color-mix(in_srgb,var(--agent-color)_6%,var(--color-surface))]',
        'border-[color-mix(in_srgb,var(--agent-color)_40%,transparent)]',
      )}
      style={{ '--agent-color': color } as React.CSSProperties}
    >
      {/* Agent header */}
      <div className="flex items-center gap-sm mb-sm">
        <span
          className="w-6 h-6 rounded-full flex items-center justify-center text-[10px] font-bold text-white shrink-0"
          style={{ backgroundColor: color }}
          aria-hidden
        >
          {source.agentName.charAt(0).toUpperCase()}
        </span>
        <span className="text-sm font-semibold text-text">{source.agentName}</span>
        {contribution && (
          <span className="text-xs text-text-muted">Round {contribution.round + 1}</span>
        )}
      </div>

      {/* Reasoning (collapsible, only while streaming) */}
      {reasoning && (
        <details className="mb-sm text-xs" open={isStreaming}>
          <summary className="cursor-pointer text-text-secondary hover:text-text transition-colors">
            Reasoning
          </summary>
          <div className="mt-xs text-text-muted whitespace-pre-wrap leading-relaxed">
            {reasoning}
          </div>
        </details>
      )}

      {/* Tool calls */}
      {toolCalls.length > 0 && (
        <div className="mb-sm flex flex-col gap-[2px]">
          {toolCalls.map((tc, i) => (
            <ToolCallBadge key={`${tc.toolName}-${i}`} call={tc} />
          ))}
        </div>
      )}

      {/* Main content */}
      {text ? (
        <MarkdownMessageContent text={text} />
      ) : isStreaming ? (
        <span className="text-sm text-text-muted animate-pulse">Speaking…</span>
      ) : null}

      {/* Core claim pill */}
      {coreClaim && <CoreClaimPill claim={coreClaim} />}
    </div>
  );
};
