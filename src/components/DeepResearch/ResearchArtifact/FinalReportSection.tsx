import React, { useMemo } from 'react';
import { CitationRef } from './CitationRef';
import type { ResearchState, GatheredFact } from './types';

interface FinalReportSectionProps {
  report: string;
  facts: GatheredFact[];
  citations: ResearchState['citations'];
}

/**
 * Final report section (when complete).
 * Renders citations [1], [2], etc. as hoverable cards showing fact details.
 */
const FinalReportSection: React.FC<FinalReportSectionProps> = ({ report, facts, citations }) => {
  // Build a map of citation number -> fact for quick lookup
  const citationToFact = useMemo(() => {
    const map = new Map<number, GatheredFact>();

    // If we have explicit citations array, use that
    if (citations && citations.length > 0) {
      citations.forEach((cit, idx) => {
        const fact = facts.find(f => f.id === cit.factId);
        if (fact) {
          map.set(idx + 1, fact);
        }
      });
    } else {
      // Fallback: assume facts are in citation order
      facts.forEach((fact, idx) => {
        map.set(idx + 1, fact);
      });
    }

    return map;
  }, [facts, citations]);

  // Parse report and replace [N] with interactive citations
  const renderedReport = useMemo(() => {
    // Match citation patterns like [1], [2], [12], etc.
    const citationRegex = /\[(\d+)\]/g;
    const parts: React.ReactNode[] = [];
    let lastIndex = 0;
    let match: RegExpExecArray | null;
    let keyIdx = 0;

    while ((match = citationRegex.exec(report)) !== null) {
      // Add text before this citation
      if (match.index > lastIndex) {
        parts.push(report.slice(lastIndex, match.index));
      }

      const citNum = parseInt(match[1], 10);
      const fact = citationToFact.get(citNum);

      if (fact) {
        // Render interactive citation with hover card
        parts.push(
          <CitationRef key={`cit-${keyIdx++}`} number={citNum} fact={fact} />
        );
      } else {
        // No fact found, render as plain text
        parts.push(match[0]);
      }

      lastIndex = match.index + match[0].length;
    }

    // Add remaining text
    if (lastIndex < report.length) {
      parts.push(report.slice(lastIndex));
    }

    return parts;
  }, [report, citationToFact]);

  return (
    <div className="p-4">
      <div className="text-sm text-text leading-relaxed whitespace-pre-wrap">{renderedReport}</div>
    </div>
  );
};

FinalReportSection.displayName = 'FinalReportSection';

export { FinalReportSection };
