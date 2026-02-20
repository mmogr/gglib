import React from 'react';
import { Brain } from 'lucide-react';
import { Icon } from '../../ui/Icon';

interface ThinkingBlockProps {
  lastReasoning: string | null;
}

/**
 * Shows the last reasoning / "thinking" from the research agent.
 */
const ThinkingBlock: React.FC<ThinkingBlockProps> = ({ lastReasoning }) => {
  if (!lastReasoning) return null;

  return (
    <div className="px-3 py-2 bg-background-secondary rounded text-xs text-text-secondary italic border-l-2 border-[#60a5fa]/30">
    <div className="flex items-center gap-1 mb-1 text-[#60a5fa] font-medium not-italic">
      <Icon icon={Brain} size={12} />
      Thinking
    </div>
    {lastReasoning}
  </div>
  );
};

ThinkingBlock.displayName = 'ThinkingBlock';

export { ThinkingBlock };
