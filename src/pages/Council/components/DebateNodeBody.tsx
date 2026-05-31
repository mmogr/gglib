/**
 * DebateNodeBody — live debate stream renderer for `debate`-kind nodes.
 *
 * Rendered by NodePanel when `node.kind` is a debate object.
 * Displays: round headers, per-agent coloured text streams, judge
 * assessment, stance map badges, and the synthesis verdict.
 *
 * Reads `nodeState.debateState` from the orchestrator reducer; no
 * direct event wiring needed here.
 *
 * @module pages/Council/components/DebateNodeBody
 */

import type { FC } from 'react';
import { CheckCircle, Gavel, Loader, TrendingDown, TrendingUp } from 'lucide-react';
import { cn } from '../../../utils/cn';
import type {
  DebateNodeState,
  DebateRoundState,
  DebateAgentTurn,
  DebateSynthesisState,
} from '../../../contexts/CouncilContext';
import type { AgentStance, StanceOutcome } from '../../../types/council';

// ─── Props ────────────────────────────────────────────────────────────────────

export interface DebateNodeBodyProps {
  nodeId: string;
  debateState: DebateNodeState;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const STANCE_LABEL: Record<StanceOutcome, string> = {
  held: 'Held',
  shifted: 'Shifted',
  conceded: 'Conceded',
};

function stanceBadgeClass(outcome: StanceOutcome): string {
  switch (outcome) {
    case 'held':
      return 'bg-primary/10 text-primary border-primary/20';
    case 'shifted':
      return 'bg-warning/10 text-warning border-warning/20';
    case 'conceded':
      return 'bg-danger/10 text-danger border-danger/20';
  }
}

function StanceIcon({ outcome }: { outcome: StanceOutcome }) {
  switch (outcome) {
    case 'held':
      return <CheckCircle className="w-3 h-3" />;
    case 'shifted':
      return <TrendingDown className="w-3 h-3" />;
    case 'conceded':
      return <TrendingUp className="w-3 h-3" />;
  }
}

// ─── AgentTurnBlock ───────────────────────────────────────────────────────────

const AgentTurnBlock: FC<{ turn: DebateAgentTurn }> = ({ turn }) => {
  const textToShow = turn.done ? (turn.finalText ?? turn.text) : turn.text;
  return (
    <div className="flex flex-col gap-xs" data-testid={`debate-agent-turn-${turn.agentId}`}>
      {/* Agent header */}
      <div className="flex items-center gap-xs">
        <span
          className="w-2 h-2 rounded-full shrink-0"
          style={{ backgroundColor: turn.color }}
        />
        <span
          className="text-xs font-semibold"
          style={{ color: turn.color }}
        >
          {turn.agentName}
        </span>
        {!turn.done && (
          <Loader className="w-3 h-3 animate-spin text-text-muted shrink-0" />
        )}
        {/* Tool log indicators */}
        {turn.toolLog.length > 0 && (
          <span className="ml-auto text-xs text-text-muted">
            {turn.toolLog.filter(t => t.done).length}/{turn.toolLog.length} tools
          </span>
        )}
      </div>
      {/* Text stream */}
      {textToShow && (
        <pre
          className={cn(
            'text-xs whitespace-pre-wrap font-mono leading-relaxed rounded-sm px-sm py-xs',
            'bg-surface border border-border/60',
            !turn.done && 'opacity-90',
          )}
          style={{ borderLeftColor: turn.color, borderLeftWidth: '2px' }}
        >
          {textToShow}
          {!turn.done && <span className="inline-block w-1 h-3 bg-current animate-pulse ml-[2px]" />}
        </pre>
      )}
    </div>
  );
};

// ─── JudgeBlock ───────────────────────────────────────────────────────────────

const JudgeBlock: FC<{ round: DebateRoundState }> = ({ round }) => {
  if (!round.judgeSummary && !round.judgeText) return null;

  return (
    <div
      className="rounded-sm border border-warning/30 bg-warning/5 p-sm flex flex-col gap-xs"
      data-testid={`debate-judge-round-${round.round}`}
    >
      <div className="flex items-center gap-xs text-xs font-semibold text-warning">
        <Gavel className="w-3 h-3" />
        Judge
        {round.judgeSummary && (
          <span className={cn(
            'ml-auto text-xs font-medium px-xs py-[2px] rounded-sm border',
            round.judgeSummary.consensusReached
              ? 'bg-success/10 text-success border-success/20'
              : 'bg-text-muted/10 text-text-muted border-border',
          )}>
            {round.judgeSummary.consensusReached ? 'Consensus' : 'No consensus'}
          </span>
        )}
      </div>

      {round.judgeSummary ? (
        <p className="text-xs text-text leading-relaxed">{round.judgeSummary.assessmentText}</p>
      ) : round.judgeText ? (
        <pre className="text-xs whitespace-pre-wrap font-mono leading-relaxed text-text">
          {round.judgeText}
          <span className="inline-block w-1 h-3 bg-current animate-pulse ml-[2px]" />
        </pre>
      ) : null}
    </div>
  );
};

// ─── RoundBlock ───────────────────────────────────────────────────────────────

const RoundBlock: FC<{ round: DebateRoundState }> = ({ round }) => {
  const turnList = Object.values(round.turns);

  return (
    <div className="flex flex-col gap-sm" data-testid={`debate-round-${round.round}`}>
      {/* Round header */}
      <div className="flex items-center gap-xs">
        <span className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
          Round {round.round}
        </span>
        {round.compacted && (
          <span className="text-xs text-text-muted italic">(compacted)</span>
        )}
        <div className="flex-1 h-px bg-border ml-xs" />
      </div>

      {round.compacted && round.compactedSummary ? (
        <p className="text-xs text-text-secondary italic leading-relaxed bg-surface-elevated rounded-sm px-sm py-xs border border-border">
          {round.compactedSummary}
        </p>
      ) : (
        <>
          {turnList.map(turn => (
            <AgentTurnBlock key={turn.agentId} turn={turn} />
          ))}
          <JudgeBlock round={round} />
        </>
      )}
    </div>
  );
};

// ─── StanceMapBlock ───────────────────────────────────────────────────────────

const StanceMapBlock: FC<{ stances: AgentStance[] }> = ({ stances }) => {
  if (stances.length === 0) return null;

  return (
    <div className="flex flex-col gap-xs" data-testid="debate-stance-map">
      <p className="text-xs text-text-muted font-medium uppercase tracking-wide">Final stances</p>
      <div className="flex flex-wrap gap-xs">
        {stances.map(s => (
          <span
            key={s.agent_id}
            className={cn(
              'flex items-center gap-xs text-xs font-medium px-sm py-[3px] rounded-sm border',
              stanceBadgeClass(s.outcome),
            )}
          >
            <StanceIcon outcome={s.outcome} />
            <span className="font-mono">{s.agent_id}</span>
            <span className="font-normal opacity-70">·</span>
            <span>{STANCE_LABEL[s.outcome]}</span>
          </span>
        ))}
      </div>
    </div>
  );
};

// ─── SynthesisBlock ───────────────────────────────────────────────────────────

const SynthesisBlock: FC<{ synthesis: DebateSynthesisState }> = ({ synthesis }) => {
  if (!synthesis.started) return null;
  const textToShow = synthesis.done ? (synthesis.finalText ?? synthesis.text) : synthesis.text;

  return (
    <div className="flex flex-col gap-xs" data-testid="debate-synthesis">
      <p className="text-xs text-text-muted font-medium uppercase tracking-wide flex items-center gap-xs">
        Synthesis verdict
        {!synthesis.done && <Loader className="w-3 h-3 animate-spin" />}
      </p>
      {textToShow && (
        <pre className="text-xs whitespace-pre-wrap font-mono leading-relaxed rounded-sm px-sm py-xs bg-surface-elevated border border-border text-text">
          {textToShow}
          {!synthesis.done && <span className="inline-block w-1 h-3 bg-current animate-pulse ml-[2px]" />}
        </pre>
      )}
    </div>
  );
};

// ─── DebateNodeBody ───────────────────────────────────────────────────────────

const DebateNodeBody: FC<DebateNodeBodyProps> = ({ nodeId: _nodeId, debateState }) => {
  return (
    <div className="flex flex-col gap-md" data-testid="debate-node-body">
      {/* Rounds */}
      {debateState.rounds.map(round => (
        <RoundBlock key={round.round} round={round} />
      ))}

      {/* Stance map — shown after rounds complete */}
      {debateState.stances.length > 0 && (
        <StanceMapBlock stances={debateState.stances} />
      )}

      {/* Synthesis */}
      <SynthesisBlock synthesis={debateState.synthesis} />
    </div>
  );
};

export default DebateNodeBody;
