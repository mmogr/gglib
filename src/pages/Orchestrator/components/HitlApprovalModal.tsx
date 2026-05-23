/**
 * HitlApprovalModal — Human-in-the-loop approval modal.
 *
 * Renders one of three variants based on ApprovalKind:
 *   - plan:  shows the full graph JSON with an optional edit textarea.
 *   - node:  shows node goal and tool allowlist.
 *   - tool:  shows the pending tool name and node context.
 *
 * On approve, calls onApprove with the appropriate ApprovalDecisionPayload.
 * On reject, calls onReject with an optional reason.
 */

import { FC, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import { Modal } from '../../../components/ui/Modal';
import { Button } from '../../../components/ui/Button';
import { Textarea } from '../../../components/ui/Textarea';
import { Icon } from '../../../components/ui/Icon';
import type {
  ApprovalKind,
  ApprovalDecisionPayload,
  TaskGraph,
} from '../../../types/orchestrator';
import type { RunCostEstimate } from '../../../contexts/OrchestratorContext';
import SteeringPanel from './SteeringPanel';

interface HitlApprovalModalProps {
  open: boolean;
  kind: ApprovalKind;
  graph: TaskGraph | null;
  submitting: boolean;
  costEstimate: RunCostEstimate | null;
  /** Advisory upper bound for the active NodeBudget (default 25). */
  budgetUpper?: number;
  /** Port for the steering LLM (required for plan edits via SteeringPanel). */
  port?: number;
  /** Optional model name for the steering LLM. */
  model?: string;
  onApprove: (payload: ApprovalDecisionPayload) => void;
  onReject: (reason?: string) => void;
}

const HitlApprovalModal: FC<HitlApprovalModalProps> = ({
  open,
  kind,
  graph,
  submitting,
  costEstimate,
  budgetUpper = 25,
  port,
  model,
  onApprove,
  onReject,
}) => {
  const [rejectReason, setRejectReason] = useState('');
  const [showReject, setShowReject] = useState(false);
  const [showEdit, setShowEdit] = useState(false);
  // Tracks the current working graph (possibly modified via SteeringPanel).
  const [editedGraph, setEditedGraph] = useState<TaskGraph | null>(null);

  function handleApprove() {
    if (showEdit && editedGraph) {
      onApprove({ decision: 'approve_with_edits', edited_graph: editedGraph });
    } else {
      onApprove({ decision: 'approve' });
    }
  }

  function handleReject() {
    onReject(rejectReason.trim() || undefined);
    setRejectReason('');
    setShowReject(false);
  }

  function handleStartEdit() {
    setShowEdit(true);
    setEditedGraph(graph);
  }

  const showCostBanner =
    costEstimate !== null &&
    (costEstimate.estWallSeconds > 60 || costEstimate.nodeCount > budgetUpper * 0.8);

  const title =
    kind.kind === 'plan'
      ? 'Approve proposed plan?'
      : kind.kind === 'node'
        ? `Approve node: ${kind.node_id}`
        : kind.kind === 'tool'
          ? `Approve tool call: ${kind.tool_name}`
          : `Approve spawn subteam for node: ${kind.node_id}`;

  const description =
    kind.kind === 'plan'
      ? 'Review the task graph and approve or reject the plan before execution begins.'
      : kind.kind === 'node'
        ? `The orchestrator wants to execute node "${kind.node_id}". Approve to allow it to run.`
        : kind.kind === 'tool'
          ? `The orchestrator wants to call "${kind.tool_name}" inside node "${kind.node_id}".`
          : `The orchestrator wants to spawn a sub-team for node "${kind.node_id}" with roles: ${kind.suggested_roles.join(', ')}.`;

  return (
    <Modal
      open={open}
      onClose={() => !submitting && onReject()}
      title={title}
      description={description}
      size="lg"
      preventClose={submitting}
      footer={
        showReject ? (
          <>
            <Button variant="secondary" size="md" onClick={() => setShowReject(false)} disabled={submitting}>
              Back
            </Button>
            <Button variant="danger" size="md" onClick={handleReject} isLoading={submitting}>
              Confirm Reject
            </Button>
          </>
        ) : (
          <>
            <Button variant="danger" size="md" onClick={() => setShowReject(true)} disabled={submitting}>
              Reject
            </Button>
            {kind.kind === 'plan' && !showEdit && (
              <Button variant="secondary" size="md" onClick={handleStartEdit} disabled={submitting}>
                Approve with Edits
              </Button>
            )}
            <Button variant="primary" size="md" onClick={handleApprove} isLoading={submitting}>
              {showEdit ? 'Approve with Edits' : 'Approve'}
            </Button>
          </>
        )
      }
    >
      <div className="flex flex-col gap-md">
        {showCostBanner && costEstimate && (
          <div
            role="alert"
            data-testid="cost-warning-banner"
            className="rounded-base border border-warning/40 bg-warning/8 px-md py-sm flex items-start gap-sm"
          >
            <Icon icon={AlertTriangle} size={15} className="text-warning shrink-0 mt-[1px]" />
            <p className="text-sm text-text-secondary">
              This plan has <strong>{costEstimate.nodeCount}</strong> node
              {costEstimate.nodeCount !== 1 ? 's' : ''} and is estimated to take
              approximately <strong>{costEstimate.estWallSeconds}s</strong>{' '}
              (~{Math.round(costEstimate.estTokens / 1000)}k tokens).{' '}
              You can still run it — this is advisory only.
            </p>
          </div>
        )}

        {showReject && (
          <div className="flex flex-col gap-sm">
            <label className="text-sm font-medium text-text">Rejection reason (optional)</label>
            <Textarea
              value={rejectReason}
              onChange={(e) => setRejectReason(e.target.value)}
              placeholder="Why are you rejecting this?"
              rows={3}
            />
          </div>
        )}

        {!showReject && kind.kind === 'plan' && graph && (
          <div className="flex flex-col gap-sm">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium text-text">Task graph</span>
              {!showEdit && (
                <span className="text-xs text-text-muted">
                  {Object.keys(graph.nodes).length} nodes · hitl_mode: {graph.hitl_mode}
                </span>
              )}
            </div>

            {showEdit ? (
              <>
                {port != null && (editedGraph ?? graph) ? (
                  <SteeringPanel
                    graph={(editedGraph ?? graph)!}
                    port={port}
                    model={model}
                    onGraphChange={(g) => setEditedGraph(g)}
                  />
                ) : (
                  <p className="text-xs text-text-muted">
                    No server port available for steering edits.
                  </p>
                )}
              </>
            ) : (
              <pre className="text-xs text-text-secondary bg-surface rounded-base p-sm overflow-x-auto max-h-[320px] overflow-y-auto scrollbar-thin font-mono">
                {JSON.stringify(graph, null, 2)}
              </pre>
            )}
          </div>
        )}

        {!showReject && kind.kind === 'node' && graph && (
          <div className="flex flex-col gap-sm">
            {graph.nodes[kind.node_id] && (
              <>
                <div>
                  <p className="text-xs text-text-muted font-medium mb-xs">Goal</p>
                  <p className="text-sm text-text">{graph.nodes[kind.node_id].goal}</p>
                </div>
                {graph.nodes[kind.node_id].tool_allowlist.length > 0 && (
                  <div>
                    <p className="text-xs text-text-muted font-medium mb-xs">Allowed tools</p>
                    <div className="flex flex-wrap gap-xs">
                      {graph.nodes[kind.node_id].tool_allowlist.map((t) => (
                        <span key={t} className="text-xs bg-surface-elevated text-text-secondary px-xs py-[2px] rounded-sm font-mono">
                          {t}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </>
            )}
          </div>
        )}

        {!showReject && kind.kind === 'tool' && (
          <div className="flex flex-col gap-sm">
            <div>
              <p className="text-xs text-text-muted font-medium mb-xs">Tool</p>
              <p className="text-sm font-mono text-text">{kind.tool_name}</p>
            </div>
            <div>
              <p className="text-xs text-text-muted font-medium mb-xs">Node</p>
              <p className="text-sm font-mono text-text-secondary">{kind.node_id}</p>
            </div>
            {graph?.nodes[kind.node_id] && (
              <div>
                <p className="text-xs text-text-muted font-medium mb-xs">Node goal</p>
                <p className="text-sm text-text">{graph.nodes[kind.node_id].goal}</p>
              </div>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
};

export default HitlApprovalModal;
