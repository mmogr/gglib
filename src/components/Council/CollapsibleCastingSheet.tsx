/**
 * CollapsibleCastingSheet — collapsed-by-default wrapper for the CastingSheet.
 *
 * Collapsed summary: chevron · team icon · "N roles" · role-icon stack (≤5) · overflow count
 * Expanded: full CastingSheet actor-card grid (the unchanged existing component)
 *
 * Internal expand/collapse state managed here — the parent does not need to
 * track it. Pass `defaultExpanded` to override the closed default, e.g. when
 * re-rendering an already-viewed run.
 *
 * @module components/Council/CollapsibleCastingSheet
 */

import { type FC, useId, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Edit3,
  GitMerge,
  MessageSquareWarning,
  PenTool,
  Search,
  ShieldCheck,
  Swords,
  User,
  Users,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '../../utils/cn';
import type { NodeState } from '../../contexts/CouncilContext';
import type { TaskGraph } from '../../types/orchestrator';
import CastingSheet, {
  collectLeafNodes,
} from '../Council/components/CastingSheet';

// ─── Role icon map (mirrors the one inside CastingSheet) ──────────────────────

const ROLE_ICON_MAP: Record<string, LucideIcon> = {
  researcher: Search,
  'red-team': Swords,
  'fact-checker': ShieldCheck,
  writer: PenTool,
  editor: Edit3,
  critic: MessageSquareWarning,
  synthesizer: GitMerge,
};

function getRoleIcon(role: string | null | undefined): LucideIcon {
  if (!role) return User;
  return ROLE_ICON_MAP[role] ?? User;
}

// ─── Props ────────────────────────────────────────────────────────────────────

export interface CollapsibleCastingSheetProps {
  graph: TaskGraph;
  nodeStates: Record<string, NodeState>;
  onSelectNode?: (nodeId: string) => void;
  selectedNodeId?: string | null;
  /** When true, the sheet opens expanded on first render. Defaults to false. */
  defaultExpanded?: boolean;
}

// Maximum role icons shown in the collapsed header before a "+N" overflow badge.
const MAX_HEADER_ICONS = 5;

// ─── Component ────────────────────────────────────────────────────────────────

/**
 * Renders a single-line summary header that expands on click to reveal the
 * full CastingSheet actor-card grid.
 *
 * The header intentionally renders only lightweight metadata (icons from a
 * local map, a role count) so it has minimal layout cost when collapsed — the
 * full CastingSheet is not mounted until the user chooses to expand it.
 */
const CollapsibleCastingSheet: FC<CollapsibleCastingSheetProps> = ({
  graph,
  nodeStates,
  onSelectNode,
  selectedNodeId,
  defaultExpanded = false,
}) => {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const headerId = useId();
  const panelId = `${headerId}-panel`;

  const leafNodes = collectLeafNodes(graph);
  const roleCount = leafNodes.length;
  const iconsToShow = leafNodes.slice(0, MAX_HEADER_ICONS);
  const overflow = roleCount - MAX_HEADER_ICONS;

  return (
    <div
      className="border border-border rounded-base overflow-hidden"
      data-testid="collapsible-casting-sheet"
    >
      {/* ── Collapsed header / toggle ──────────────────────────────────────── */}
      <button
        type="button"
        id={headerId}
        onClick={() => setIsExpanded((v) => !v)}
        aria-expanded={isExpanded}
        aria-controls={panelId}
        className={cn(
          'w-full flex items-center gap-sm px-sm py-xs text-left',
          'bg-surface hover:bg-surface-hover transition-colors',
          isExpanded && 'border-b border-border',
        )}
        data-testid="collapsible-casting-sheet-toggle"
      >
        {/* Chevron */}
        {isExpanded ? (
          <ChevronDown size={14} className="text-text-muted shrink-0" aria-hidden="true" />
        ) : (
          <ChevronRight size={14} className="text-text-muted shrink-0" aria-hidden="true" />
        )}

        {/* Team icon */}
        <Users size={13} className="text-primary shrink-0" aria-hidden="true" />

        {/* Label */}
        <span className="text-xs font-medium text-text-secondary tabular-nums">
          Team&nbsp;&middot;&nbsp;{roleCount}&nbsp;{roleCount === 1 ? 'role' : 'roles'}
        </span>

        {/* Role icon stack — hidden to screen readers, purely decorative */}
        {roleCount > 0 && (
          <div
            className="flex items-center -space-x-1 ml-xs"
            aria-hidden="true"
            data-testid="role-icon-stack"
          >
            {iconsToShow.map(({ nodeId, node }) => {
              const RoleIcon = getRoleIcon(node.role);
              return (
                <span
                  key={nodeId}
                  className={cn(
                    'flex items-center justify-center w-5 h-5 rounded-full shrink-0',
                    'bg-surface-elevated border border-border',
                  )}
                >
                  <RoleIcon size={10} className="text-text-muted" />
                </span>
              );
            })}
            {overflow > 0 && (
              <span
                className={cn(
                  'flex items-center justify-center w-5 h-5 rounded-full shrink-0',
                  'bg-surface-elevated border border-border',
                  'text-[9px] text-text-muted font-medium',
                )}
              >
                +{overflow}
              </span>
            )}
          </div>
        )}

        {/* Spacer so the chevron anchors to the left and the rest hugs left */}
        <span className="flex-1" />
      </button>

      {/* ── Expanded panel — full CastingSheet ────────────────────────────── */}
      {isExpanded && (
        <div
          id={panelId}
          role="region"
          aria-labelledby={headerId}
          className="p-sm"
        >
          <CastingSheet
            graph={graph}
            nodeStates={nodeStates}
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
          />
        </div>
      )}
    </div>
  );
};

export default CollapsibleCastingSheet;
