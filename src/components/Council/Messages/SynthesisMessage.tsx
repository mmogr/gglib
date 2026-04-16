/**
 * Renders the council synthesis — the final consensus output.
 *
 * Visually distinct from agent turns: no temperature tinting, uses a
 * neutral/primary accent with subtle elevation to signal authority.
 *
 * @module components/Council/Messages/SynthesisMessage
 */

import type { FC } from 'react';
import { Sparkles } from 'lucide-react';
import { cn } from '../../../utils/cn';
import { Icon } from '../../ui/Icon';
import MarkdownMessageContent from '../../ChatMessagesPanel/components/MarkdownMessageContent';

export interface SynthesisMessageProps {
  text: string;
  isStreaming?: boolean;
}

export const SynthesisMessage: FC<SynthesisMessageProps> = ({ text, isStreaming }) => (
  <div
    className={cn(
      'rounded-base border p-md transition-colors duration-200',
      'border-primary/30 bg-primary/[0.04]',
      'shadow-[0_0_8px_0_var(--color-primary,#6366f1)_inset,0_0_0_1px_var(--color-primary,#6366f1)_inset] shadow-primary/[0.06]',
    )}
  >
    {/* Badge */}
    <div className="flex items-center gap-sm mb-sm">
      <span className="w-6 h-6 rounded-full flex items-center justify-center bg-primary/20 shrink-0">
        <Icon icon={Sparkles} size={14} className="text-primary" />
      </span>
      <span className="text-sm font-semibold text-primary">Council Synthesis</span>
    </div>

    {/* Content */}
    {text ? (
      <MarkdownMessageContent text={text} />
    ) : isStreaming ? (
      <span className="text-sm text-text-muted animate-pulse">Synthesizing consensus…</span>
    ) : null}
  </div>
);
