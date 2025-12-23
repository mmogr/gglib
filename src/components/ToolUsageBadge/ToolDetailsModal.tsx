import React, { useState, useEffect } from 'react';
import type { ThreadMessage } from '@assistant-ui/react';
import { Check, CheckCircle2, Clipboard, Loader2, Wrench, X, XCircle, ChevronRight } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Modal } from '../ui/Modal';
import styles from './ToolDetailsModal.module.css';

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
      <div className={styles.heading}>
        <span className={styles.headingIcon}>
          <Icon icon={Wrench} size={16} />
        </span>
        <div>
          <p className={styles.headingTitle}>Tool calls</p>
          <p className={styles.headingSubtitle}>Inspect arguments and results from each tool execution.</p>
        </div>
      </div>

      <div className={styles.content}>
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
            <div key={call.toolCallId || index} className={styles.toolCard}>
              <div className={styles.toolHeader}>
                <span className={`${styles.statusIcon} ${StatusIcon === XCircle ? styles.statusError : StatusIcon === Loader2 ? styles.statusPending : styles.statusSuccess}`}>
                  <Icon icon={StatusIcon} size={16} className={StatusIcon === Loader2 ? styles.spinner : ''} />
                </span>
                <span className={styles.toolName}>{formatToolName(call.toolName)}</span>
                <span className={styles.toolNameRaw}>({call.toolName})</span>
              </div>

              <div className={styles.section}>
                <div className={styles.sectionHeader} onClick={() => toggleSection(argsId)} role="button" tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      toggleSection(argsId);
                    }
                  }}
                >
                  <ChevronRight className={`${styles.chevron} ${argsExpanded ? styles.chevronExpanded : ''}`} size={14} />
                  <span className={styles.sectionTitle}>Arguments</span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={styles.copyButton}
                    onClick={(e) => {
                      e.stopPropagation();
                      copyToClipboard(formattedArgs, argsId);
                    }}
                    leftIcon={<Icon icon={copiedId === argsId ? Check : Clipboard} size={14} />}
                  >
                    {copiedId === argsId ? 'Copied' : 'Copy'}
                  </Button>
                </div>
                {argsExpanded && <pre className={styles.jsonContent}>{formattedArgs}</pre>}
              </div>

              <div className={styles.section}>
                <div className={styles.sectionHeader} onClick={() => toggleSection(resultId)} role="button" tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      toggleSection(resultId);
                    }
                  }}
                >
                  <ChevronRight className={`${styles.chevron} ${resultExpanded ? styles.chevronExpanded : ''}`} size={14} />
                  <span className={styles.sectionTitle}>Result</span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={styles.copyButton}
                    onClick={(e) => {
                      e.stopPropagation();
                      copyToClipboard(formattedResult, resultId);
                    }}
                    leftIcon={<Icon icon={copiedId === resultId ? Check : Clipboard} size={14} />}
                  >
                    {copiedId === resultId ? 'Copied' : 'Copy'}
                  </Button>
                </div>
                {resultExpanded && <pre className={styles.jsonContent}>{formattedResult}</pre>}
              </div>
            </div>
          );
        })}
      </div>

      <div className={styles.footerActions}>
        <Button variant="ghost" onClick={onClose} rightIcon={<Icon icon={X} size={14} />}>
          Close
        </Button>
      </div>
    </Modal>
  );
};

export default ToolDetailsModal;
