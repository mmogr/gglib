/**
 * SteeringPanel — chat-style input for conversational graph steering.
 *
 * Accepts a natural-language instruction, sends it to
 * `POST /api/orchestrator/steer` for a diff preview, renders the diff with
 * visual encoding (green = add, red = remove, amber = reroute/modify), and
 * provides an "Apply diff" button that calls `onGraphChange`.
 *
 * @module pages/Orchestrator/components/SteeringPanel
 */

import { FC, useState } from 'react';
import { Button } from '../../../components/ui/Button';
import { Textarea } from '../../../components/ui/Textarea';
import type { TaskGraph, GraphDiff, TaskNode } from '../../../types/orchestrator';

// ─── Pure helper: apply a GraphDiff to a TaskGraph ───────────────────────────

/**
 * Apply `diff` to `graph` and return the updated graph.
 * This is a best-effort client-side application for preview; the authoritative
 * application happens in the Rust `apply_diff` implementation.
 */
export function applyDiff(graph: TaskGraph, diff: GraphDiff): TaskGraph {
  const nodes = { ...graph.nodes };

  switch (diff.op) {
    case 'add_node': {
      nodes[diff.node.id] = diff.node;
      break;
    }
    case 'remove_node': {
      delete nodes[diff.id];
      // Strip edges pointing to removed node.
      for (const id of Object.keys(nodes)) {
        nodes[id] = {
          ...nodes[id],
          depends_on: nodes[id].depends_on.filter((d) => d !== diff.id),
        };
      }
      break;
    }
    case 'split_node': {
      delete nodes[diff.id];
      for (const n of diff.into) {
        nodes[n.id] = n;
      }
      // Repoint dependants.
      for (const id of Object.keys(nodes)) {
        if (nodes[id].depends_on.includes(diff.id)) {
          nodes[id] = {
            ...nodes[id],
            depends_on: [
              ...nodes[id].depends_on.filter((d) => d !== diff.id),
              ...diff.into.map((n) => n.id),
            ],
          };
        }
      }
      break;
    }
    case 'reroute_edge': {
      const node = nodes[diff.node_id];
      if (node) {
        nodes[diff.node_id] = {
          ...node,
          depends_on: node.depends_on.map((d) => (d === diff.old_dep ? diff.new_dep : d)),
        };
      }
      break;
    }
    case 'set_role': {
      // Role is not part of the TaskNode TS interface but keep for completeness.
      break;
    }
    case 'set_tools': {
      const node = nodes[diff.id];
      if (node) {
        nodes[diff.id] = { ...node, tool_allowlist: diff.tool_allowlist };
      }
      break;
    }
    case 'wrap_in_team': {
      // Simplified: remove wrapped ids and add team node.
      for (const id of diff.ids) {
        delete nodes[id];
      }
      const teamNode: TaskNode = {
        id: diff.team_id,
        goal: diff.team_goal,
        depends_on: [],
        tool_allowlist: [],
        status: 'pending',
      };
      nodes[diff.team_id] = teamNode;
      break;
    }
  }

  return { ...graph, nodes };
}

// ─── Diff summary rendering ───────────────────────────────────────────────────

interface DiffBadgeProps {
  label: string;
  color: 'green' | 'red' | 'amber';
}

const DiffBadge: FC<DiffBadgeProps> = ({ label, color }) => {
  const cls =
    color === 'green'
      ? 'bg-success/15 text-success border-success/30'
      : color === 'red'
        ? 'bg-danger/15 text-danger border-danger/30'
        : 'bg-warning/15 text-warning border-warning/30';

  return (
    <span
      className={`inline-flex items-center rounded px-2 py-0.5 text-xs font-medium border ${cls}`}
    >
      {label}
    </span>
  );
};

function DiffPreview({ diff }: { diff: GraphDiff }) {
  switch (diff.op) {
    case 'add_node':
      return (
        <div className="flex items-start gap-sm">
          <DiffBadge label="ADD" color="green" />
          <span className="text-sm text-text-secondary">
            Node <code className="font-mono">{diff.node.id}</code>: {diff.node.goal}
          </span>
        </div>
      );
    case 'remove_node':
      return (
        <div className="flex items-start gap-sm">
          <DiffBadge label="REMOVE" color="red" />
          <span className="text-sm text-text-secondary">
            Node <code className="font-mono">{diff.id}</code>
          </span>
        </div>
      );
    case 'split_node':
      return (
        <div className="flex items-start gap-sm">
          <DiffBadge label="SPLIT" color="amber" />
          <span className="text-sm text-text-secondary">
            Node <code className="font-mono">{diff.id}</code> →{' '}
            {diff.into.map((n) => n.id).join(', ')}
          </span>
        </div>
      );
    case 'reroute_edge':
      return (
        <div className="flex items-start gap-sm">
          <DiffBadge label="REROUTE" color="amber" />
          <span className="text-sm text-text-secondary">
            <code className="font-mono">{diff.node_id}</code> dep{' '}
            <code className="font-mono">{diff.old_dep}</code> →{' '}
            <code className="font-mono">{diff.new_dep}</code>
          </span>
        </div>
      );
    case 'set_role':
      return (
        <div className="flex items-start gap-sm">
          <DiffBadge label="ROLE" color="amber" />
          <span className="text-sm text-text-secondary">
            Node <code className="font-mono">{diff.id}</code> role →{' '}
            {diff.role ?? 'none'}
          </span>
        </div>
      );
    case 'set_tools':
      return (
        <div className="flex items-start gap-sm">
          <DiffBadge label="TOOLS" color="amber" />
          <span className="text-sm text-text-secondary">
            Node <code className="font-mono">{diff.id}</code> allowlist →{' '}
            [{diff.tool_allowlist.join(', ')}]
          </span>
        </div>
      );
    case 'wrap_in_team':
      return (
        <div className="flex items-start gap-sm">
          <DiffBadge label="WRAP TEAM" color="green" />
          <span className="text-sm text-text-secondary">
            [{diff.ids.join(', ')}] →{' '}
            <code className="font-mono">{diff.team_id}</code>: {diff.team_goal}
          </span>
        </div>
      );
    default:
      return <span className="text-xs text-text-muted">(unknown diff op)</span>;
  }
}

// ─── SteeringPanel ────────────────────────────────────────────────────────────

export interface SteeringPanelProps {
  graph: TaskGraph;
  port: number;
  model?: string;
  onGraphChange: (newGraph: TaskGraph) => void;
}

const SteeringPanel: FC<SteeringPanelProps> = ({ graph, port, model, onGraphChange }) => {
  const [instruction, setInstruction] = useState('');
  const [pendingDiff, setPendingDiff] = useState<GraphDiff | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handlePreview() {
    const trimmed = instruction.trim();
    if (!trimmed) return;

    setLoading(true);
    setError(null);
    setPendingDiff(null);

    try {
      const res = await fetch('/api/orchestrator/steer', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ graph, instruction: trimmed, port, model }),
      });

      if (!res.ok) {
        const text = await res.text();
        setError(`Request failed (${res.status}): ${text}`);
        return;
      }

      const body = (await res.json()) as { diff: GraphDiff };
      setPendingDiff(body.diff);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }

  function handleApply() {
    if (!pendingDiff) return;
    onGraphChange(applyDiff(graph, pendingDiff));
    setPendingDiff(null);
    setInstruction('');
  }

  return (
    <div className="flex flex-col gap-md" data-testid="steering-panel">
      <div className="flex flex-col gap-sm">
        <label className="text-sm font-medium text-text">Steering instruction</label>
        <Textarea
          value={instruction}
          onChange={(e) => setInstruction(e.target.value)}
          placeholder="e.g. Split the 'research' node into two parallel sub-tasks…"
          rows={3}
          aria-label="Steering instruction"
        />
      </div>

      <div className="flex items-center gap-sm">
        <Button
          variant="primary"
          size="md"
          onClick={handlePreview}
          isLoading={loading}
          disabled={!instruction.trim() || loading}
        >
          Preview diff
        </Button>
        {pendingDiff && (
          <Button
            variant="secondary"
            size="md"
            onClick={() => setPendingDiff(null)}
            disabled={loading}
          >
            Discard
          </Button>
        )}
      </div>

      {error && (
        <p className="text-sm text-danger" role="alert">
          {error}
        </p>
      )}

      {pendingDiff && (
        <div
          className="flex flex-col gap-sm rounded-base border border-border bg-surface p-md"
          data-testid="diff-preview"
        >
          <span className="text-xs font-medium text-text-muted uppercase tracking-wide">
            Proposed diff
          </span>
          <DiffPreview diff={pendingDiff} />
          <Button variant="primary" size="sm" onClick={handleApply}>
            Apply diff
          </Button>
        </div>
      )}
    </div>
  );
};

export default SteeringPanel;
