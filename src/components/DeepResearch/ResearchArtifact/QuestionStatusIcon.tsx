import React from 'react';
import { Circle, Loader2, CircleCheck, CircleX } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import type { QuestionStatus } from './types';

interface QuestionStatusIconProps {
  status: QuestionStatus;
}

/**
 * Get question status icon.
 */
const QuestionStatusIcon: React.FC<QuestionStatusIconProps> = ({ status }) => {
  switch (status) {
    case 'pending':
      return <Icon icon={Circle} size={16} className="text-text-muted" />;
    case 'in-progress':
      return <Icon icon={Loader2} size={16} className="text-[#60a5fa] animate-research-pulse" />;
    case 'answered':
      return <Icon icon={CircleCheck} size={16} className="text-[#4ade80]" />;
    case 'blocked':
      return <Icon icon={CircleX} size={16} className="text-[#f87171]" />;
  }
};

QuestionStatusIcon.displayName = 'QuestionStatusIcon';

export { QuestionStatusIcon };
