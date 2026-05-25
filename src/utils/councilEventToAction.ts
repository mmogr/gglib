/**
 * councilEventToAction — canonical mapping from a raw CouncilEvent SSE payload
 * to a typed OrchestratorAction for the orchestrator reducer.
 *
 * Single source of truth shared by:
 *   - hooks/useCouncil/useCouncil.ts
 *   - components/Council/Thread/useCouncilRunStream.ts
 *   - components/Council/Thread/HistoricalCouncilThread.tsx
 *
 * Returns null for informational events that have no reducer effect.
 *
 * @module utils/councilEventToAction
 */

import type { OrchestratorAction } from '../contexts/CouncilContext';
import type { CouncilEvent } from '../types/council';

export function councilEventToAction(event: CouncilEvent): OrchestratorAction | null {
  switch (event.type) {
    case 'plan_proposed':
      return { type: 'PLAN_PROPOSED', graph: event.graph };
    case 'run_cost_estimate':
      return {
        type: 'SET_COST_ESTIMATE',
        nodeCount: event.node_count,
        estTokens: event.est_tokens,
        estWallSeconds: event.est_wall_seconds,
      };
    case 'plan_approved':
      return { type: 'PLAN_APPROVED' };
    case 'plan_rejected':
      return { type: 'PLAN_REJECTED', reason: event.reason };
    case 'replan_attempt':
      return { type: 'REPLAN_ATTEMPT', attempt: event.attempt, reason: event.reason };
    case 'awaiting_approval':
      return { type: 'AWAITING_APPROVAL', approvalId: event.approval_id, kind: event.kind };
    case 'node_started':
      return { type: 'NODE_STARTED', nodeId: event.node_id, goal: event.goal };
    case 'node_text_delta':
      return { type: 'NODE_TEXT_DELTA', nodeId: event.node_id, delta: event.delta };
    case 'node_tool_call_start':
      return {
        type: 'NODE_TOOL_CALL_START',
        nodeId: event.node_id,
        displayName: event.display_name,
        argsSummary: event.args_summary,
      };
    case 'node_tool_call_complete':
      return {
        type: 'NODE_TOOL_CALL_COMPLETE',
        nodeId: event.node_id,
        toolName: event.tool_name,
        displayName: event.display_name,
        durationDisplay: event.duration_display,
      };
    case 'node_compacting':
      return { type: 'NODE_COMPACTING', nodeId: event.node_id };
    case 'node_complete':
      return { type: 'NODE_COMPLETE', nodeId: event.node_id, outputPreview: event.output_preview };
    case 'node_failed':
      return { type: 'NODE_FAILED', nodeId: event.node_id, error: event.error };
    case 'synthesis_start':
      return { type: 'SYNTHESIS_START' };
    case 'synthesis_text_delta':
      return { type: 'SYNTHESIS_TEXT_DELTA', delta: event.delta };
    case 'synthesis_complete':
      return { type: 'SYNTHESIS_COMPLETE', content: event.content };
    case 'orchestrator_complete':
      return { type: 'ORCHESTRATOR_COMPLETE', answer: event.answer };
    case 'orchestrator_error':
      return { type: 'ORCHESTRATOR_ERROR', message: event.message };
    case 'steering_applied':
      return { type: 'SET_PENDING_DIFF', diff: event.diff };
    case 'debate_round_started':
      return { type: 'DEBATE_ROUND_STARTED', nodeId: event.node_id, round: event.round };
    case 'debate_agent_turn_started':
      return {
        type: 'DEBATE_AGENT_TURN_STARTED',
        nodeId: event.node_id, agentId: event.agent_id,
        agentName: event.agent_name, color: event.color,
        round: event.round, contentiousness: event.contentiousness,
      };
    case 'debate_agent_text_delta':
      return { type: 'DEBATE_AGENT_TEXT_DELTA', nodeId: event.node_id, agentId: event.agent_id, delta: event.delta };
    case 'debate_agent_tool_call_start':
      return {
        type: 'DEBATE_AGENT_TOOL_CALL_START',
        nodeId: event.node_id, agentId: event.agent_id,
        displayName: event.display_name, argsSummary: event.args_summary,
      };
    case 'debate_agent_tool_call_complete':
      return {
        type: 'DEBATE_AGENT_TOOL_CALL_COMPLETE',
        nodeId: event.node_id, agentId: event.agent_id,
        displayName: event.display_name, durationDisplay: event.duration_display,
      };
    case 'debate_agent_turn_complete':
      return {
        type: 'DEBATE_AGENT_TURN_COMPLETE',
        nodeId: event.node_id, agentId: event.agent_id,
        round: event.round, finalText: event.final_text,
      };
    case 'debate_judge_started':
      return { type: 'DEBATE_JUDGE_STARTED', nodeId: event.node_id, round: event.round };
    case 'debate_judge_text_delta':
      return { type: 'DEBATE_JUDGE_TEXT_DELTA', nodeId: event.node_id, delta: event.delta };
    case 'debate_judge_summary':
      return {
        type: 'DEBATE_JUDGE_SUMMARY',
        nodeId: event.node_id, round: event.round,
        consensusReached: event.consensus_reached,
        earlyStopRecommended: event.early_stop_recommended,
        assessmentText: event.assessment_text,
      };
    case 'debate_round_compacted':
      return { type: 'DEBATE_ROUND_COMPACTED', nodeId: event.node_id, round: event.round, summary: event.summary };
    case 'debate_stance_map':
      return { type: 'DEBATE_STANCE_MAP', nodeId: event.node_id, stances: event.stances };
    case 'debate_synthesis_started':
      return { type: 'DEBATE_SYNTHESIS_STARTED', nodeId: event.node_id };
    case 'debate_synthesis_text_delta':
      return { type: 'DEBATE_SYNTHESIS_TEXT_DELTA', nodeId: event.node_id, delta: event.delta };
    case 'debate_synthesis_complete':
      return { type: 'DEBATE_SYNTHESIS_COMPLETE', nodeId: event.node_id, finalText: event.final_text };
    // Informational events — no reducer effect
    case 'debate_agent_reasoning_delta':
    case 'node_reasoning_delta':
    case 'node_progress':
    case 'node_system_warning':
    case 'synthesis_progress':
    case 'team_started':
    case 'team_synthesized':
    case 'subteam_spawned':
    case 'wave_completed':
      return null;
    default:
      return null;
  }
}
