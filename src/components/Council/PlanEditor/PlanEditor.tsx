/**
 * PlanEditor — visual editor for an in-flight task graph.
 *
 * Shown during the `awaiting_approval` phase when `kind.kind === 'plan'`.
 * Lets the user inspect the proposed DAG, apply structured edits (role
 * changes, tool overrides, node additions / removals, dependency rewiring),
 * and then either approve the plan (with or without edits) or reject it with
 * an optional free-text reason.
 *
 * ## Architecture
 * - All mutations are tracked via `usePlanEditor` (undo-stack + structural
 *   sharing — no in-place mutation).
 * - On approve the component serialises `draft.current` as `edited_graph`
 *   only when `isDirty`; otherwise it sends a plain `{ decision: 'approve' }`.
 * - `DagView` is used read-only for spatial layout; clicking a node selects
 *   it into the `NodeEditPane`.
 *
 * @module components/Council/PlanEditor/PlanEditor
 */

import { type FC, useCallback, useId, useMemo, useState } from 'react';
import {
  AlertTriangle,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Clock,
  GitMerge,
  History,
  Network,
  Plus,
  RotateCcw,
  Trash2,
  Undo2,
  XCircle,
} from 'lucide-react';

import { cn } from '../../../utils/cn';
import type { ApprovalDecisionPayload, TaskGraph, TaskNode, DebateConfig } from '../../../types/council';
import type { RunCostEstimate } from '../../../contexts/CouncilContext';
import type { NodeState } from '../../../contexts/CouncilContext';
import DagView from '../../../pages/Council/components/DagView';
import { usePlanEditor } from './usePlanEditor';
import { newDiffId } from '../../../types/graph-diff';
import DebateRosterEditor from './DebateRosterEditor';

// ─── Props ────────────────────────────────────────────────────────────────────

export interface PlanEditorProps {
  /** The proposed TaskGraph from the `plan_proposed` SSE event. */
  graph: TaskGraph;
  onApprove: (payload: ApprovalDecisionPayload) => void;
  onReject: (reason?: string) => void;
  costEstimate?: RunCostEstimate | null;
  /** Whether the approve/reject calls are in-flight. */
  submitting?: boolean;
  className?: string;
}

// ─── Empty node states for the DAG view (pre-run) ────────────────────────────
// All nodes are `pending` before any wave runs; passing empty record is fine
// because DagView defaults to pending for missing entries.
const EMPTY_NODE_STATES: Record<string, NodeState> = {};

// ─── Helper: build a blank new node ──────────────────────────────────────────
function blankNode(): TaskNode {
  return {
    id: newDiffId(),
    goal: '',
    depends_on: [],
    tool_allowlist: [],
    status: 'pending',
    role: null,
  };
}

// ─── Sub-component: DiffChangelog ────────────────────────────────────────────

interface DiffChangelogProps {
  diffs: { id: string; description: string; timestamp: number }[];
  onUndo: () => void;
  onReset: () => void;
  canUndo: boolean;
}

const DiffChangelog: FC<DiffChangelogProps> = ({ diffs, onUndo, onReset, canUndo }) => {
  const [open, setOpen] = useState(false);
  const headerId = useId();

  if (diffs.length === 0) return null;

  return (
    <div
      className="border-t border-border shrink-0"
      data-testid="plan-editor-diff-changelog"
    >
      <button
        type="button"
        className="w-full flex items-center gap-xs px-sm py-xs text-xs text-text-secondary hover:text-text hover:bg-surface-hover transition-colors"
        onClick={() => setOpen(o => !o)}
        aria-expanded={open}
        aria-controls={headerId}
      >
        {open ? (
          <ChevronDown className="w-3 h-3 shrink-0" />
        ) : (
          <ChevronRight className="w-3 h-3 shrink-0" />
        )}
        <History className="w-3 h-3 shrink-0" />
        <span className="font-medium">Changes</span>
        <span className="ml-xs bg-primary/15 text-primary rounded-full px-1 leading-4">
          {diffs.length}
        </span>
        <span className="ml-auto flex items-center gap-xs">
          <button
            type="button"
            onClick={e => { e.stopPropagation(); onUndo(); }}
            disabled={!canUndo}
            className="flex items-center gap-xs text-xs text-text-secondary hover:text-text disabled:opacity-40 disabled:cursor-not-allowed"
            aria-label="Undo last change"
          >
            <Undo2 className="w-3 h-3" />
            Undo
          </button>
          <button
            type="button"
            onClick={e => { e.stopPropagation(); onReset(); }}
            disabled={!canUndo}
            className="flex items-center gap-xs text-xs text-danger/80 hover:text-danger disabled:opacity-40 disabled:cursor-not-allowed"
            aria-label="Reset all changes"
          >
            <RotateCcw className="w-3 h-3" />
            Reset all
          </button>
        </span>
      </button>

      {open && (
        <ol
          id={headerId}
          className="max-h-32 overflow-y-auto scrollbar-thin px-sm py-xs space-y-[2px]"
          aria-label="Change history"
        >
          {[...diffs].reverse().map((d, i) => (
            <li key={d.id} className="flex items-center gap-xs text-xs text-text-secondary">
              <span className="text-text-muted tabular-nums w-4 text-right shrink-0">
                {diffs.length - i}
              </span>
              <span className="truncate">{d.description}</span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
};

// ─── Sub-component: NodeEditPane ─────────────────────────────────────────────

interface NodeEditPaneProps {
  node: TaskNode;
  allNodeIds: string[];
  onApplyGoal: (id: string, goal: string) => void;
  onApplyRole: (id: string, role: string | null) => void;
  onApplyTools: (id: string, tools: string[]) => void;
  onApplyDeps: (id: string, deps: string[]) => void;
  onRemove: (id: string) => void;
}

const NodeEditPane: FC<NodeEditPaneProps> = ({
  node,
  allNodeIds,
  onApplyGoal,
  onApplyRole,
  onApplyTools,
  onApplyDeps,
  onRemove,
}) => {
  const [goal, setGoal] = useState(node.goal);
  const [role, setRole] = useState(node.role ?? '');
  // Tools as a comma-separated string — a proper multi-select is TODO Phase 4.
  const [toolsRaw, setToolsRaw] = useState(node.tool_allowlist.join(', '));
  const goalId = useId();
  const roleId = useId();
  const toolsId = useId();
  const depsId = useId();

  const handleGoalBlur = () => {
    const trimmed = goal.trim();
    if (trimmed && trimmed !== node.goal) {
      onApplyGoal(node.id, trimmed);
    }
  };

  const handleRoleBlur = () => {
    const trimmed = role.trim() || null;
    if (trimmed !== (node.role ?? null)) {
      onApplyRole(node.id, trimmed);
    }
  };

  const handleToolsBlur = () => {
    const parsed = toolsRaw
      .split(',')
      .map(t => t.trim())
      .filter(Boolean);
    const current = [...node.tool_allowlist].sort().join(',');
    const next = [...parsed].sort().join(',');
    if (current !== next) {
      onApplyTools(node.id, parsed);
    }
  };

  const toggleDep = (depId: string) => {
    const next = node.depends_on.includes(depId)
      ? node.depends_on.filter(d => d !== depId)
      : [...node.depends_on, depId];
    onApplyDeps(node.id, next);
  };

  const candidateDeps = allNodeIds.filter(id => id !== node.id);

  return (
    <div
      className="flex flex-col gap-sm p-sm overflow-y-auto scrollbar-thin flex-1 min-h-0"
      data-testid="plan-editor-node-pane"
    >
      {/* Node ID pill */}
      <div className="flex items-center justify-between gap-xs">
        <span className="text-xs font-mono text-text-muted truncate">{node.id}</span>
        <button
          type="button"
          onClick={() => onRemove(node.id)}
          className="flex items-center gap-xs text-xs text-danger/70 hover:text-danger transition-colors shrink-0"
          aria-label={`Remove node ${node.id}`}
          data-testid="plan-editor-remove-node"
        >
          <Trash2 className="w-3 h-3" />
          Remove node
        </button>
      </div>

      {/* Goal */}
      <div className="flex flex-col gap-xs">
        <label htmlFor={goalId} className="text-xs font-medium text-text-secondary">
          Goal
        </label>
        <textarea
          id={goalId}
          rows={3}
          className="text-sm bg-surface border border-border rounded-base px-sm py-xs text-text placeholder:text-text-muted resize-y focus:outline-none focus:border-primary/50 transition-colors"
          value={goal}
          onChange={e => setGoal(e.target.value)}
          onBlur={handleGoalBlur}
          placeholder="Describe what this node should accomplish…"
          data-testid="plan-editor-goal-input"
        />
      </div>

      {/* Role */}
      <div className="flex flex-col gap-xs">
        <label htmlFor={roleId} className="text-xs font-medium text-text-secondary">
          Role
          <span className="ml-xs text-text-muted font-normal">(optional)</span>
        </label>
        <input
          id={roleId}
          type="text"
          className="text-sm bg-surface border border-border rounded-base px-sm py-xs text-text placeholder:text-text-muted focus:outline-none focus:border-primary/50 transition-colors"
          value={role}
          onChange={e => setRole(e.target.value)}
          onBlur={handleRoleBlur}
          placeholder="e.g. researcher, critic, writer…"
          data-testid="plan-editor-role-input"
        />
      </div>

      {/* Tools — TODO Phase 4: replace with multi-select from tool registry */}
      <div className="flex flex-col gap-xs">
        <label htmlFor={toolsId} className="text-xs font-medium text-text-secondary">
          Tool allowlist
          <span className="ml-xs text-text-muted font-normal">(comma-separated)</span>
        </label>
        <input
          id={toolsId}
          type="text"
          className="text-sm bg-surface border border-border rounded-base px-sm py-xs text-text placeholder:text-text-muted focus:outline-none focus:border-primary/50 transition-colors"
          value={toolsRaw}
          onChange={e => setToolsRaw(e.target.value)}
          onBlur={handleToolsBlur}
          placeholder="e.g. web_search, read_file, …"
          data-testid="plan-editor-tools-input"
        />
      </div>

      {/* Dependencies */}
      {candidateDeps.length > 0 && (
        <div className="flex flex-col gap-xs">
          <span id={depsId} className="text-xs font-medium text-text-secondary">
            Depends on
          </span>
          <ul role="list" aria-labelledby={depsId} className="space-y-[3px]">
            {candidateDeps.map(depId => (
              <li key={depId}>
                <label className="flex items-center gap-xs text-xs text-text cursor-pointer group">
                  <input
                    type="checkbox"
                    className="accent-primary"
                    checked={node.depends_on.includes(depId)}
                    onChange={() => toggleDep(depId)}
                  />
                  <span className="font-mono text-text-muted truncate group-hover:text-text transition-colors">
                    {depId}
                  </span>
                </label>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
};

// ─── Sub-component: AddNodePane ───────────────────────────────────────────────

interface AddNodePaneProps {
  allNodeIds: string[];
  onAdd: (node: TaskNode) => void;
  onCancel: () => void;
}

const AddNodePane: FC<AddNodePaneProps> = ({ allNodeIds, onAdd, onCancel }) => {
  const [node, setNode] = useState<TaskNode>(blankNode);
  const [goalError, setGoalError] = useState('');
  const goalId = useId();
  const roleId = useId();

  const handleSubmit = () => {
    const trimmedGoal = node.goal.trim();
    if (!trimmedGoal) {
      setGoalError('Goal is required');
      return;
    }
    setGoalError('');
    onAdd({ ...node, goal: trimmedGoal });
  };

  return (
    <div
      className="flex flex-col gap-sm p-sm overflow-y-auto scrollbar-thin flex-1 min-h-0"
      data-testid="plan-editor-add-node-pane"
    >
      <p className="text-xs font-medium text-text-secondary flex items-center gap-xs">
        <Plus className="w-3 h-3" />
        New node
      </p>

      {/* Goal */}
      <div className="flex flex-col gap-xs">
        <label htmlFor={goalId} className="text-xs font-medium text-text-secondary">
          Goal <span className="text-danger">*</span>
        </label>
        <textarea
          id={goalId}
          rows={3}
          autoFocus
          className={cn(
            'text-sm bg-surface border rounded-base px-sm py-xs text-text placeholder:text-text-muted resize-y focus:outline-none transition-colors',
            goalError ? 'border-danger focus:border-danger' : 'border-border focus:border-primary/50',
          )}
          value={node.goal}
          onChange={e => setNode(n => ({ ...n, goal: e.target.value }))}
          placeholder="What should this node do?"
          data-testid="plan-editor-new-goal"
        />
        {goalError && (
          <p className="text-xs text-danger">{goalError}</p>
        )}
      </div>

      {/* Role */}
      <div className="flex flex-col gap-xs">
        <label htmlFor={roleId} className="text-xs font-medium text-text-secondary">
          Role <span className="text-text-muted font-normal">(optional)</span>
        </label>
        <input
          id={roleId}
          type="text"
          className="text-sm bg-surface border border-border rounded-base px-sm py-xs text-text placeholder:text-text-muted focus:outline-none focus:border-primary/50 transition-colors"
          value={node.role ?? ''}
          onChange={e => setNode(n => ({ ...n, role: e.target.value || null }))}
          placeholder="e.g. researcher"
          data-testid="plan-editor-new-role"
        />
      </div>

      {/* Depends on */}
      {allNodeIds.length > 0 && (
        <div className="flex flex-col gap-xs">
          <span className="text-xs font-medium text-text-secondary">Depends on</span>
          <ul role="list" className="space-y-[3px]">
            {allNodeIds.map(depId => (
              <li key={depId}>
                <label className="flex items-center gap-xs text-xs text-text cursor-pointer">
                  <input
                    type="checkbox"
                    className="accent-primary"
                    checked={node.depends_on.includes(depId)}
                    onChange={() =>
                      setNode(n => ({
                        ...n,
                        depends_on: n.depends_on.includes(depId)
                          ? n.depends_on.filter(d => d !== depId)
                          : [...n.depends_on, depId],
                      }))
                    }
                  />
                  <span className="font-mono text-text-muted truncate">{depId}</span>
                </label>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="flex gap-xs mt-auto pt-xs border-t border-border shrink-0">
        <button
          type="button"
          onClick={handleSubmit}
          className="flex-1 text-xs font-medium py-xs rounded-base bg-primary text-white hover:bg-primary/90 transition-colors"
          data-testid="plan-editor-add-node-confirm"
        >
          Add node
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="flex-1 text-xs font-medium py-xs rounded-base border border-border text-text-secondary hover:text-text hover:bg-surface-hover transition-colors"
        >
          Cancel
        </button>
      </div>
    </div>
  );
};

// ─── Main component ───────────────────────────────────────────────────────────

const PlanEditor: FC<PlanEditorProps> = ({
  graph,
  onApprove,
  onReject,
  costEstimate,
  submitting = false,
  className,
}) => {
  const { draft, applyOp, undo, reset, canUndo } = usePlanEditor(graph);

  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [addingNode, setAddingNode] = useState(false);
  const [rejectMode, setRejectMode] = useState(false);
  const [rejectReason, setRejectReason] = useState('');

  const currentNodes = draft.current.nodes;
  const nodeIds = useMemo(
    () => Object.keys(currentNodes),
    [currentNodes],
  );

  const selectedNode = selectedNodeId != null
    ? draft.current.nodes[selectedNodeId] ?? null
    : null;

  // ── Op callbacks ────────────────────────────────────────────────────────────

  const handleApplyGoal = useCallback(
    (id: string, goal: string) => applyOp({ op: 'set_goal', id, goal }),
    [applyOp],
  );
  const handleApplyRole = useCallback(
    (id: string, role: string | null) => applyOp({ op: 'set_role', id, role }),
    [applyOp],
  );
  const handleApplyTools = useCallback(
    (id: string, tool_allowlist: string[]) =>
      applyOp({ op: 'set_tools', id, tool_allowlist }),
    [applyOp],
  );
  const handleApplyDeps = useCallback(
    (id: string, depends_on: string[]) => applyOp({ op: 'set_deps', id, depends_on }),
    [applyOp],
  );
  const handleApplyDebateConfig = useCallback(
    (id: string, config: DebateConfig) => applyOp({ op: 'set_debate_config', id, config }),
    [applyOp],
  );
  const handleRemove = useCallback(
    (id: string) => {
      applyOp({ op: 'remove_node', id });
      setSelectedNodeId(prev => (prev === id ? null : prev));
    },
    [applyOp],
  );
  const handleAddNode = useCallback(
    (node: TaskNode) => {
      applyOp({ op: 'add_node', node });
      setAddingNode(false);
      setSelectedNodeId(node.id);
    },
    [applyOp],
  );

  // ── Select node ─────────────────────────────────────────────────────────────

  const handleSelectNode = useCallback((nodeId: string) => {
    setAddingNode(false);
    setSelectedNodeId(prev => (prev === nodeId ? null : nodeId));
  }, []);

  // ── Approve / Reject ────────────────────────────────────────────────────────

  const handleApprove = () => {
    if (draft.isDirty) {
      onApprove({ decision: 'approve_with_edits', edited_graph: draft.current });
    } else {
      onApprove({ decision: 'approve' });
    }
  };

  const handleRejectConfirm = () => {
    onReject(rejectReason.trim() || undefined);
  };

  // ─────────────────────────────────────────────────────────────────────────────

  return (
    <div
      className={cn('flex flex-col h-full overflow-hidden bg-background', className)}
      data-testid="plan-editor"
    >
      {/* ── Header ─────────────────────────────────────────────────────────── */}
      <div className="flex items-center gap-sm px-md py-sm border-b border-border shrink-0 min-w-0">
        <GitMerge className="w-4 h-4 text-text-secondary shrink-0" />
        <span className="text-sm font-semibold text-text truncate flex-1" title={draft.current.goal}>
          {draft.current.goal}
        </span>

        {/* Change count badge */}
        {draft.isDirty && (
          <span
            className="flex items-center gap-xs text-xs font-medium px-xs py-[2px] rounded-sm bg-warning/10 text-warning shrink-0"
            aria-label={`${draft.applied.length} pending changes`}
          >
            <History className="w-3 h-3" />
            {draft.applied.length} {draft.applied.length === 1 ? 'change' : 'changes'}
          </span>
        )}

        {/* Cost estimate */}
        {costEstimate && (
          <span className="hidden sm:flex items-center gap-xs text-xs text-text-muted shrink-0">
            <Clock className="w-3 h-3" />
            ~{Math.round(costEstimate.estWallSeconds)}s
            · {costEstimate.nodeCount} nodes
          </span>
        )}

        {/* Node count */}
        <span className="flex items-center gap-xs text-xs text-text-muted shrink-0">
          <Network className="w-3 h-3" />
          {nodeIds.length}
        </span>

        {/* Reject */}
        <button
          type="button"
          onClick={() => { setRejectMode(r => !r); }}
          disabled={submitting}
          className="flex items-center gap-xs text-xs font-medium px-sm py-xs rounded-base border border-border text-text-secondary hover:text-danger hover:border-danger/50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors shrink-0"
          data-testid="plan-editor-reject-btn"
        >
          <XCircle className="w-3 h-3" />
          Reject
        </button>

        {/* Approve */}
        <button
          type="button"
          onClick={handleApprove}
          disabled={submitting}
          className={cn(
            'flex items-center gap-xs text-xs font-medium px-sm py-xs rounded-base transition-colors shrink-0 disabled:opacity-40 disabled:cursor-not-allowed',
            draft.isDirty
              ? 'bg-warning text-white hover:bg-warning/90'
              : 'bg-success text-white hover:bg-success/90',
          )}
          data-testid="plan-editor-approve-btn"
        >
          {draft.isDirty ? (
            <>
              <AlertTriangle className="w-3 h-3" />
              Approve with edits
            </>
          ) : (
            <>
              <CheckCircle className="w-3 h-3" />
              Approve
            </>
          )}
        </button>
      </div>

      {/* ── Reject inline form ──────────────────────────────────────────────── */}
      {rejectMode && (
        <div
          className="flex items-center gap-sm px-md py-sm border-b border-danger/30 bg-danger/5 shrink-0"
          data-testid="plan-editor-reject-form"
        >
          <input
            type="text"
            className="flex-1 text-sm bg-surface border border-danger/30 rounded-base px-sm py-xs text-text placeholder:text-text-muted focus:outline-none focus:border-danger/60 transition-colors"
            placeholder="Optional reason for rejection…"
            value={rejectReason}
            onChange={e => setRejectReason(e.target.value)}
            onKeyDown={e => { if (e.key === 'Enter') handleRejectConfirm(); }}
            autoFocus
            data-testid="plan-editor-reject-reason"
            aria-label="Rejection reason"
          />
          <button
            type="button"
            onClick={handleRejectConfirm}
            disabled={submitting}
            className="text-xs font-medium px-sm py-xs rounded-base bg-danger text-white hover:bg-danger/90 disabled:opacity-40 transition-colors shrink-0"
            data-testid="plan-editor-reject-confirm"
          >
            Confirm reject
          </button>
          <button
            type="button"
            onClick={() => setRejectMode(false)}
            className="text-xs text-text-secondary hover:text-text transition-colors shrink-0"
            aria-label="Cancel rejection"
          >
            Cancel
          </button>
        </div>
      )}

      {/* ── Body ───────────────────────────────────────────────────────────── */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left: DAG view */}
        <div
          className="flex-1 min-w-0 overflow-y-auto scrollbar-thin border-r border-border"
          data-testid="plan-editor-dag"
        >
          <div className="p-sm">
            <DagView
              graph={draft.current}
              nodeStates={EMPTY_NODE_STATES}
              selectedNodeId={selectedNodeId}
              onSelectNode={handleSelectNode}
            />
          </div>
        </div>

        {/* Right: Edit / Add pane */}
        <div
          className="w-72 flex flex-col min-h-0 overflow-hidden bg-surface shrink-0"
          data-testid="plan-editor-side"
        >
          {/* Add node button */}
          <div className="px-sm py-xs border-b border-border shrink-0">
            <button
              type="button"
              onClick={() => { setAddingNode(true); setSelectedNodeId(null); }}
              className="w-full flex items-center justify-center gap-xs text-xs font-medium py-xs rounded-base border border-dashed border-border text-text-secondary hover:text-primary hover:border-primary/40 transition-colors"
              data-testid="plan-editor-add-node-btn"
            >
              <Plus className="w-3 h-3" />
              Add node
            </button>
          </div>

          {/* Pane body */}
          {addingNode ? (
            <AddNodePane
              allNodeIds={nodeIds}
              onAdd={handleAddNode}
              onCancel={() => setAddingNode(false)}
            />
          ) : selectedNode && selectedNode.kind != null && typeof selectedNode.kind === 'object' && 'debate' in selectedNode.kind ? (
            <DebateRosterEditor
              key={selectedNode.id}
              nodeId={selectedNode.id}
              config={selectedNode.kind.debate.config}
              onApplyConfig={handleApplyDebateConfig}
            />
          ) : selectedNode ? (
            // Key on node ID so state resets when a different node is selected.
            <NodeEditPane
              key={selectedNode.id}
              node={selectedNode}
              allNodeIds={nodeIds}
              onApplyGoal={handleApplyGoal}
              onApplyRole={handleApplyRole}
              onApplyTools={handleApplyTools}
              onApplyDeps={handleApplyDeps}
              onRemove={handleRemove}
            />
          ) : (
            <div
              className="flex flex-col items-center justify-center flex-1 gap-sm text-text-muted text-xs p-md text-center"
              data-testid="plan-editor-empty-prompt"
            >
              <Network className="w-8 h-8 opacity-30" />
              <p>Click a node in the graph to edit its goal, role, tools, or dependencies.</p>
            </div>
          )}

          {/* Change log */}
          <DiffChangelog
            diffs={draft.applied}
            onUndo={undo}
            onReset={reset}
            canUndo={canUndo}
          />
        </div>
      </div>
    </div>
  );
};

export default PlanEditor;
