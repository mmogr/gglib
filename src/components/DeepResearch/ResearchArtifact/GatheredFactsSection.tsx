import React, { useState } from 'react';
import { FileSearch, ExternalLink, CheckCircle2, AlertTriangle, XCircle } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import type { GatheredFact } from './types';

interface GatheredFactsSectionProps {
  facts: GatheredFact[];
}

/**
 * Gathered facts section — shows discovered facts with confidence badges and source links.
 */
const GatheredFactsSection: React.FC<GatheredFactsSectionProps> = ({ facts }) => {
  const [showAll, setShowAll] = useState(false);
  const displayLimit = 5;

  if (facts.length === 0) {
    return (
      <div className="px-3.5 py-3 border-b border-border last:border-b-0">
        <div className="flex items-center justify-between mb-2.5">
          <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
            <Icon icon={FileSearch} size={14} />
            Gathered Facts
          </span>
        </div>
        <div className="text-center p-4 text-text-muted text-xs italic">No facts gathered yet...</div>
      </div>
    );
  }

  // Sort by most recent first
  const sortedFacts = [...facts].sort((a, b) => b.gatheredAtStep - a.gatheredAtStep);
  const displayedFacts = showAll ? sortedFacts : sortedFacts.slice(0, displayLimit);

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div className="flex items-center justify-between mb-2.5">
        <span className="flex items-center gap-1.5 text-xs font-semibold text-text uppercase tracking-[0.5px]">
          <Icon icon={FileSearch} size={14} />
          Gathered Facts
        </span>
        <span className="text-[11px] text-text-muted font-normal">{facts.length} facts</span>
      </div>
      <div className="flex flex-col gap-2">
        {displayedFacts.map(fact => (
          <div
            key={fact.id}
            className="px-2.5 py-2 bg-background-tertiary rounded-md border-l-[3px] border-l-transparent data-[confidence=high]:border-l-[#4ade80] data-[confidence=medium]:border-l-[#facc15] data-[confidence=low]:border-l-[#f87171]"
            data-confidence={fact.confidence}
          >
            <div className="text-[13px] text-text leading-[1.4]">{fact.claim}</div>
            <div className="flex items-center gap-2 mt-1.5 text-[11px] text-text-muted">
              <a
                href={fact.sourceUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-1 text-[#60a5fa] no-underline max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap hover:underline"
                title={fact.sourceUrl}
              >
                <Icon icon={ExternalLink} size={10} />
                {fact.sourceTitle || new URL(fact.sourceUrl).hostname}
              </a>
              <span className="flex items-center gap-1">
                {fact.confidence === 'high' && <Icon icon={CheckCircle2} size={10} />}
                {fact.confidence === 'medium' && <Icon icon={AlertTriangle} size={10} />}
                {fact.confidence === 'low' && <Icon icon={XCircle} size={10} />}
                {fact.confidence}
              </span>
            </div>
          </div>
        ))}
      </div>
      {facts.length > displayLimit && (
        <button
          className="block w-full mt-2 px-3 py-1.5 bg-background-tertiary border border-border rounded text-text-secondary text-xs cursor-pointer transition-all duration-200 ease-out hover:bg-background-hover hover:text-text"
          onClick={() => setShowAll(!showAll)}
        >
          {showAll ? 'Show less' : `Show ${facts.length - displayLimit} more`}
        </button>
      )}
    </div>
  );
};

GatheredFactsSection.displayName = 'GatheredFactsSection';

export { GatheredFactsSection };
