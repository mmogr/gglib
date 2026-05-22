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
import { Modal } from '../../../components/ui/Modal';
import { Button } from '../../../components/ui/Button';
import { Textarea } from '../../../components/ui/Textarea';
import type {
  ApprovalKind,
  ApprovalDecisionPayload,
  TaskGraph,
} from '../../../types/orchestrator';

interface HitlApprovalModalProps {
  open: boolean;
  kind: ApprovalKind;
  graph: TaskGraph | null;
  submitting: boolean;
  onApprove: (payload: ApprovalDecisionPayload) => void;
  onReject: (reason?: string) => void;
}

const HitlApprovalModal: FC<HitlApprovalModalProps> = ({
  open,
  kind,
  graph,
  submitting,
  onApprove,
  onReject,
}) => {
  const [rejectReason, setRejectReason] = useState('');
  const [showReject, setShowReject] = useState(false);
  const [graphEditJson, setGraphEditJson] = useState('');
  const [graphEditError, setGraphEditError] = useState<string | null>(null);
  const [showEdit, setShowEdit] = useState(false);

  function handleApprove() {
    if (showEdit && graphEditJson.trim()) {
      try {
        const editedGraph = JSON.parse(graphEditJson) as TaskGraph;
        onApprove({ decision: 'approve_with_edits', edited_graph: editedGraph });
      } catch {
        setGraphEditError('Invalid JSON — please fix before approving with edits.');
        return;
      }
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
    setGraphEditJson(JSON.stringify(graph, null, 2));
    setGraphEditError(null);
  }

  const title =
    kind.kind === 'plan'
      ? 'Approve proposed plan?'
      : kind.kind === 'node'
        ? `Approve node: ${kind.node_id}`
        : `Approve tool call: ${kind.tool_name}`;

  const description =
    kind.kind === 'plan'
      ? 'Review the task graph and approve or reject the plan before execution begins.'
      : kind.kind === 'node'
        ? `The orchestrator wants to execute node "${kind.node_id}". Approve to allow it to run.`
        : `The orchestrator wants to call "${kind.tool_name}" inside node "${kind.node_id}".`;

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
                <Textarea
                  value={graphEditJson}
                  onChange={(e) => {
                    setGraphEditJson(e.target.value);
                    setGraphEditError(null);
                  }}
                  rows={16}
                  className="font-mono text-xs"
                  aria-label="Edited graph JSON"
                />
                {graphEditError && (
                  <p className="text-xs text-danger">{graphEditError}</p>
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
