import React from 'react';
import { Search, AlertTriangle } from 'lucide-react';
import { Icon } from '../../ui/Icon';

interface KnowledgeGapsSectionProps {
  gaps: string[];
}

/**
 * Knowledge gaps section — shows identified gaps as warning badges.
 */
const KnowledgeGapsSection: React.FC<KnowledgeGapsSectionProps> = ({ gaps }) => {
  if (gaps.length === 0) return null;

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div className="flex items-center justify-between mb-2.5">
        <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
          <Icon icon={Search} size={14} />
          Knowledge Gaps
        </span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {gaps.map((gap, idx) => (
          <span key={idx} className="inline-flex items-center gap-1 px-2 py-1 bg-[rgba(234,179,8,0.1)] border border-[rgba(234,179,8,0.3)] rounded text-[11px] text-[#facc15]">
            <Icon icon={AlertTriangle} size={10} />
            {gap}
          </span>
        ))}
      </div>
    </div>
  );
};

KnowledgeGapsSection.displayName = 'KnowledgeGapsSection';

export { KnowledgeGapsSection };
