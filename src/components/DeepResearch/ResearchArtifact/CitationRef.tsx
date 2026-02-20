import React, { useMemo } from 'react';
import { ExternalLink, CheckCircle2 } from 'lucide-react';
import type { GatheredFact } from './types';

interface CitationRefProps {
  number: number;
  fact: GatheredFact;
}

/**
 * Individual citation reference with hover card popover.
 * Shows [N] inline, and on hover reveals a card with fact details + source link.
 */
const CitationRef: React.FC<CitationRefProps> = ({ number, fact }) => {
  // Truncate claim for display
  const displayClaim =
    fact.claim.length > 200 ? `${fact.claim.slice(0, 200)}...` : fact.claim;

  // Get hostname for display
  const hostname = useMemo(() => {
    try {
      return new URL(fact.sourceUrl).hostname.replace(/^www\./, '');
    } catch {
      return fact.sourceUrl;
    }
  }, [fact.sourceUrl]);

  return (
    <span className="group/cite relative inline cursor-pointer text-[#60a5fa] font-semibold text-[0.85em] align-super px-0.5 rounded-sm transition-all duration-150 ease-out hover:bg-[rgba(96,165,250,0.15)] hover:text-[#93c5fd]" tabIndex={0} role="button">
      [{number}]
      <span className="absolute bottom-[calc(100%+8px)] left-1/2 -translate-x-1/2 w-[320px] max-w-[90vw] bg-background-tertiary border border-[#444] rounded-lg p-3 shadow-[0_4px_20px_rgba(0,0,0,0.4)] z-[1000] opacity-0 invisible transition-[opacity,visibility] duration-150 ease-out pointer-events-none group-hover/cite:opacity-100 group-hover/cite:visible group-hover/cite:pointer-events-auto group-focus/cite:opacity-100 group-focus/cite:visible group-focus/cite:pointer-events-auto after:content-[''] after:absolute after:top-full after:left-1/2 after:-translate-x-1/2 after:border-[6px] after:border-transparent after:border-t-border">
        <div className="flex items-start gap-2 mb-2">
          <span className="flex items-center justify-center w-5 h-5 rounded bg-[rgba(96,165,250,0.2)] text-[#60a5fa] text-[11px] font-bold shrink-0">{number}</span>
          <span className="font-semibold text-text text-[13px] leading-[1.3] flex-1 min-w-0 overflow-hidden text-ellipsis line-clamp-2">{fact.sourceTitle}</span>
        </div>
        <div className="text-xs text-text-secondary leading-normal mb-2 p-2 bg-background-secondary rounded border-l-2 border-l-[rgba(96,165,250,0.5)]">{displayClaim}</div>
        <div className="flex items-center justify-between gap-2 text-[11px]">
          <span
            className="flex items-center gap-1 text-text-muted data-[confidence=high]:text-[#4ade80] data-[confidence=medium]:text-[#fbbf24] data-[confidence=low]:text-[#f87171]"
            data-confidence={fact.confidence}
          >
            <CheckCircle2 size={12} />
            {fact.confidence} confidence
          </span>
          <a
            href={fact.sourceUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1 text-[#60a5fa] no-underline text-[11px] max-w-[150px] overflow-hidden text-ellipsis whitespace-nowrap hover:underline"
            onClick={(e) => e.stopPropagation()}
          >
            <ExternalLink size={10} />
            {hostname}
          </a>
        </div>
      </span>
    </span>
  );
};

CitationRef.displayName = 'CitationRef';

export { CitationRef };
