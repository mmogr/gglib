/**
 * CastingSheet — actor-card grid view of the planned team.
 *
 * Renders one card per leaf node (across all nesting levels) showing:
 *   - Role icon (Lucide) and role name
 *   - Mission / goal
 *   - Tool allowlist chips
 *   - Dependency chips
 *   - Ancestor team label (if inside a Team subgraph)
 *
 * Clicking a card selects the node (calls `onSelectNode`).
 *
 * @module pages/Orchestrator/components/CastingSheet
 */

import { type FC } from 'react';
import {
  Search,
  Swords,
  ShieldCheck,
  PenTool,
  Edit3,
  MessageSquareWarning,
  GitMerge,
  User,
  Wrench,
  ArrowRight,
  Users,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '../../../utils/cn';
import type { TaskGraph, TaskNode, TaskNodeKind } from '../../../types/orchestrator';
import type { NodeState, NodePhase } from '../../../contexts/CouncilContext';

// ─── Role icon map ────────────────────────────────────────────────────────────

const ROLE_ICON_MAP: Record<string, LucideIcon> = {
  researcher: Search,
  'red-team': Swords,
  'fact-checker': ShieldCheck,
  writer: PenTool,
  editor: Edit3,
  critic: MessageSquareWarning,
  synthesizer: GitMerge,
};

const ROLE_LABEL_MAP: Record<string, string> = {
  researcher: 'Researcher',
  'red-team': 'Red Team',
  'fact-checker': 'Fact Checker',
  writer: 'Writer',
  editor: 'Editor',
  critic: 'Critic',
  synthesizer: 'Synthesizer',
};

function roleIcon(role: string | null | undefined): LucideIcon {
  if (!role) return User;
  return ROLE_ICON_MAP[role] ?? User;
}

function roleLabel(role: string | null | undefined): string {
  if (!role) return 'Generic';
  return ROLE_LABEL_MAP[role] ?? role;
}

// ─── Graph traversal ─────────────────────────────────────────────────────────

export interface ActorCard {
  nodeId: string;
  node: TaskNode;
  ancestorTeamId?: string;
}

function isTeam(kind: TaskNodeKind | null | undefined): boolean {
  return typeof kind === 'object' && kind !== null && 'team' in kind;
}

/** Recursively collect all leaf nodes, tagging each with its ancestor team id. */
export function collectLeafNodes(graph: TaskGraph, ancestorTeamId?: string): ActorCard[] {
  const cards: ActorCard[] = [];
  for (const [id, node] of Object.entries(graph.nodes)) {
    if (isTeam(node.kind)) {
      const teamKind = node.kind as { team: { subgraph: TaskGraph } };
      cards.push(...collectLeafNodes(teamKind.team.subgraph, id));
    } else {
      cards.push({ nodeId: id, node, ancestorTeamId });
    }
  }
  return cards;
}

// ─── Phase badge ─────────────────────────────────────────────────────────────

function phaseRingClass(phase: NodePhase): string {
  switch (phase) {
    case 'running':
      return 'ring-1 ring-primary/40 bg-primary/5';
    case 'compacting':
      return 'ring-1 ring-warning/40 bg-warning/5';
    case 'done':
      return 'ring-1 ring-success/40 bg-success/5';
    case 'failed':
      return 'ring-1 ring-danger/40 bg-danger/5';
    default:
      return 'ring-1 ring-border bg-surface';
  }
}

// ─── ActorCard component ──────────────────────────────────────────────────────

interface ActorCardProps {
  card: ActorCard;
  nodeState: NodeState | undefined;
  isSelected: boolean;
  onSelect: () => void;
}

const ActorCardItem: FC<ActorCardProps> = ({ card, nodeState, isSelected, onSelect }) => {
  const { nodeId, node, ancestorTeamId } = card;
  const phase: NodePhase = nodeState?.phase ?? 'pending';
  const RoleIcon = roleIcon(node.role);

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-label={`Select node ${nodeId}`}
      aria-pressed={isSelected}
      className={cn(
        'flex flex-col gap-sm p-md rounded-base border text-left transition-all cursor-pointer w-full',
        isSelected ? 'border-primary/50 bg-primary/8' : phaseRingClass(phase),
        'hover:border-primary/40 hover:bg-primary/5',
      )}
      data-testid={`casting-card-${nodeId}`}
    >
      {/* Role row */}
      <div className="flex items-center gap-sm">
        <span
          className="flex items-center justify-center w-7 h-7 rounded-full bg-surface-elevated shrink-0"
          aria-hidden="true"
          data-testid={`role-icon-${node.role ?? 'generic'}`}
        >
          <RoleIcon size={14} className="text-text-secondary" />
        </span>
        <div className="flex-1 min-w-0">
          <p className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            {roleLabel(node.role)}
          </p>
          <p className="text-xs text-text-muted font-mono truncate">{nodeId}</p>
        </div>
        {phase === 'running' && (
          <span className="w-2 h-2 rounded-full bg-primary animate-pulse shrink-0" />
        )}
        {phase === 'done' && (
          <span className="text-success" aria-label="Done">✓</span>
        )}
        {phase === 'failed' && (
          <span className="text-danger" aria-label="Failed">✗</span>
        )}
      </div>

      {/* Mission */}
      <p className="text-sm text-text leading-relaxed line-clamp-3">{node.goal}</p>

      {/* Ancestor team */}
      {ancestorTeamId && (
        <div className="flex items-center gap-xs">
          <Users size={10} className="text-text-muted shrink-0" />
          <span className="text-xs text-text-muted font-mono">
            team: {ancestorTeamId}
          </span>
        </div>
      )}

      {/* Tool chips */}
      {node.tool_allowlist.length > 0 && (
        <div className="flex flex-wrap gap-xs" aria-label="Allowed tools">
          {node.tool_allowlist.map((t) => (
            <span
              key={t}
              className="flex items-center gap-[3px] text-xs bg-surface-elevated text-text-muted px-xs py-[2px] rounded-sm font-mono"
            >
              <Wrench size={9} aria-hidden="true" />
              {t}
            </span>
          ))}
        </div>
      )}

      {/* Dependency chips */}
      {node.depends_on.length > 0 && (
        <div className="flex flex-wrap gap-xs" aria-label="Dependencies">
          {node.depends_on.map((dep) => (
            <span
              key={dep}
              className="flex items-center gap-[3px] text-xs bg-surface text-text-muted border border-border px-xs py-[2px] rounded-sm font-mono"
            >
              <ArrowRight size={9} aria-hidden="true" />
              {dep}
            </span>
          ))}
        </div>
      )}
    </button>
  );
};

// ─── CastingSheet ─────────────────────────────────────────────────────────────

export interface CastingSheetProps {
  graph: TaskGraph;
  nodeStates: Record<string, NodeState>;
  selectedNodeId?: string | null;
  onSelectNode?: (nodeId: string) => void;
}

const CastingSheet: FC<CastingSheetProps> = ({ graph, nodeStates, selectedNodeId, onSelectNode }) => {
  const cards = collectLeafNodes(graph);

  if (cards.length === 0) {
    return (
      <p className="text-sm text-text-muted italic" data-testid="casting-empty">
        No leaf nodes in this graph.
      </p>
    );
  }

  return (
    <div
      className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-sm"
      data-testid="casting-sheet"
      aria-label="Casting sheet"
    >
      {cards.map(({ nodeId, node, ancestorTeamId }) => (
        <ActorCardItem
          key={nodeId}
          card={{ nodeId, node, ancestorTeamId }}
          nodeState={nodeStates[nodeId]}
          isSelected={selectedNodeId === nodeId}
          onSelect={() => onSelectNode?.(nodeId)}
        />
      ))}
    </div>
  );
};

export default CastingSheet;
