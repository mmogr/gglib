import React, { useState, useEffect } from 'react';
import type { ThreadMessage } from '@assistant-ui/react';
import { Check, CheckCircle2, Clipboard, Loader2, Wrench, X, XCircle, ChevronRight } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Modal } from '../ui/Modal';
import { cn } from '../../utils/cn';

type ToolCallPart = Extract<ThreadMessage['content'][number], { type: 'tool-call' }>;

interface ToolDetailsModalProps {
  toolCalls: ToolCallPart[];
  isOpen?: boolean;
  onClose: () => void;
}

/**
 * Modal displaying detailed information about tool executions.
 * Shows tool name, arguments, and results in expandable sections.
 */
const ToolDetailsModal: React.FC<ToolDetailsModalProps> = ({ toolCalls, isOpen = true, onClose }) => {
  const [expandedSections, setExpandedSections] = useState<Set<string>>(new Set());
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Close on ESC key
  useEffect(() => {
    const handleEsc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', handleEsc);
    return () => window.removeEventListener('keydown', handleEsc);
  }, [onClose]);

  const toggleSection = (id: string) => {
    setExpandedSections((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    });
  };

  const formatToolName = (name: string): string => {
    // Remove mcp_ prefix and format
    const cleaned = name.replace(/^mcp_\d+_/, '');
    return cleaned
      .split(/[-_]/)
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  };

  const getStatusIcon = (call: ToolCallPart): typeof CheckCircle2 => {
    if ('result' in call) {
      const result = call.result as any;
      if (result && typeof result === 'object') {
        if ('error' in result || result.success === false) {
          return XCircle;
        }
      }
      return CheckCircle2;
    }
    return Loader2;
  };

  if (!isOpen) return null;

  return (
    <Modal open={isOpen} onClose={onClose} title="Tool execution details" size="lg">
      <div className="flex gap-sm items-start mb-md">
        <span className="w-9 h-9 rounded-full inline-flex items-center justify-center bg-background-tertiary text-primary border border-border">
          <Icon icon={Wrench} size={16} />
        </span>
        <div>
          <p className="m-0 text-text font-semibold">Tool calls</p>
          <p className="mt-[0.2rem] mb-0 text-text-secondary text-[0.95rem]">Inspect arguments and results from each tool execution.</p>
        </div>
      </div>

      <div className="flex flex-col gap-sm max-h-[55vh] overflow-auto pr-[2px]">
        {toolCalls.map((call, index) => {
          const argsId = `args-${index}`;
          const resultId = `result-${index}`;
          const argsExpanded = expandedSections.has(argsId);
          const resultExpanded = expandedSections.has(resultId);

          const formattedArgs = JSON.stringify(call.args, null, 2);
          const result = 'result' in call ? call.result : null;
          const formattedResult = result ? JSON.stringify(result, null, 2) : 'No result';

          const StatusIcon = getStatusIcon(call);

          return (
            <div key={call.toolCallId || index} className="bg-background-secondary border border-border rounded-[10px] p-md flex flex-col gap-sm">
              <div className="flex items-center gap-sm">
                <span className={cn(
                  'w-5 h-5 inline-flex items-center justify-center rounded-full bg-background-tertiary border border-border text-text',
                  StatusIcon === XCircle && 'text-[#ef4444] border-[rgba(239,68,68,0.35)]',
                  StatusIcon === CheckCircle2 && 'text-[#16a34a] border-[rgba(22,163,74,0.35)]',
                  StatusIcon === Loader2 && 'text-text-secondary',
                )}>
                  <Icon icon={StatusIcon} size={16} className={StatusIcon === Loader2 ? 'animate-spin' : ''} />
                </span>
                <span className="font-semibold text-text">{formatToolName(call.toolName)}</span>
                <span className="text-[0.85rem] text-text-secondary font-mono">({call.toolName})</span>
              </div>

              <div className="flex flex-col gap-xs">
                <div className="flex items-center gap-xs w-full bg-background border border-border rounded-lg py-2 px-3 cursor-pointer transition-[border-color,background] duration-150 hover:border-primary hover:bg-background-tertiary" onClick={() => toggleSection(argsId)} role="button" tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      toggleSection(argsId);
                    }
                  }}
                >
                  <ChevronRight className={cn('text-text-secondary transition-transform duration-200', argsExpanded && 'rotate-90')} size={14} />
                  <span className="flex-1 text-left font-semibold text-text">Arguments</span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="ml-auto text-text-secondary"
                    onClick={(e) => {
                      e.stopPropagation();
                      copyToClipboard(formattedArgs, argsId);
                    }}
                    leftIcon={<Icon icon={copiedId === argsId ? Check : Clipboard} size={14} />}
                  >
                    {copiedId === argsId ? 'Copied' : 'Copy'}
                  </Button>
                </div>
                {argsExpanded && <pre className="m-0 p-3 bg-background border border-border rounded-lg font-mono text-[0.9rem] leading-normal text-text overflow-x-auto whitespace-pre max-h-[300px]">{formattedArgs}</pre>}
              </div>

              <div className="flex flex-col gap-xs">
                <div className="flex items-center gap-xs w-full bg-background border border-border rounded-lg py-2 px-3 cursor-pointer transition-[border-color,background] duration-150 hover:border-primary hover:bg-background-tertiary" onClick={() => toggleSection(resultId)} role="button" tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      toggleSection(resultId);
                    }
                  }}
                >
                  <ChevronRight className={cn('text-text-secondary transition-transform duration-200', resultExpanded && 'rotate-90')} size={14} />
                  <span className="flex-1 text-left font-semibold text-text">Result</span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="ml-auto text-text-secondary"
                    onClick={(e) => {
                      e.stopPropagation();
                      copyToClipboard(formattedResult, resultId);
                    }}
                    leftIcon={<Icon icon={copiedId === resultId ? Check : Clipboard} size={14} />}
                  >
                    {copiedId === resultId ? 'Copied' : 'Copy'}
                  </Button>
                </div>
                {resultExpanded && <pre className="m-0 p-3 bg-background border border-border rounded-lg font-mono text-[0.9rem] leading-normal text-text overflow-x-auto whitespace-pre max-h-[300px]">{formattedResult}</pre>}
              </div>
            </div>
          );
        })}
      </div>

      <div className="mt-md flex justify-end">
        <Button variant="ghost" onClick={onClose} rightIcon={<Icon icon={X} size={14} />}>
          Close
        </Button>
      </div>
    </Modal>
  );
};

export default ToolDetailsModal;
