import React from 'react';
import { Lightbulb } from 'lucide-react';
import { Icon } from '../../ui/Icon';

interface HypothesisBlockProps {
  hypothesis: string | null;
}

/**
 * Working hypothesis preview block.
 */
const HypothesisBlock: React.FC<HypothesisBlockProps> = ({ hypothesis }) => {
  if (!hypothesis) return null;

  return (
    <div className="px-3.5 py-3 border-b border-border last:border-b-0">
      <div className="px-3.5 py-3 bg-background rounded-md">
        <div className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.5px] text-text-muted mb-2">
          <Icon icon={Lightbulb} size={12} />
          <span>Working Hypothesis</span>
        </div>
        <div className="text-[13px] text-text leading-normal">{hypothesis}</div>
      </div>
    </div>
  );
};

HypothesisBlock.displayName = 'HypothesisBlock';

export { HypothesisBlock };
