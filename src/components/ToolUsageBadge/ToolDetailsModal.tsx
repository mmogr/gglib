import React, { useState, useEffect } from 'react';
import type { ThreadMessage } from '@assistant-ui/react';
import styles from './ToolDetailsModal.module.css';

type ToolCallPart = Extract<ThreadMessage['content'][number], { type: 'tool-call' }>;

interface ToolDetailsModalProps {
  toolCalls: ToolCallPart[];
  onClose: () => void;
}

/**
 * Modal displaying detailed information about tool executions.
 * Shows tool name, arguments, and results in expandable sections.
 */
const ToolDetailsModal: React.FC<ToolDetailsModalProps> = ({ toolCalls, onClose }) => {
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

  const getStatusIcon = (call: ToolCallPart): string => {
    if ('result' in call) {
      const result = call.result as any;
      if (result && typeof result === 'object') {
        if ('error' in result || result.success === false) {
          return '❌';
        }
      }
      return '✅';
    }
    return '⏳';
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <div className={styles.header}>
          <h2 className={styles.title}>Tool Execution Details</h2>
          <button className={styles.closeButton} onClick={onClose} title="Close">
            ✕
          </button>
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

            return (
              <div key={call.toolCallId || index} className={styles.toolCard}>
                <div className={styles.toolHeader}>
                  <span className={styles.statusIcon}>{getStatusIcon(call)}</span>
                  <span className={styles.toolName}>{formatToolName(call.toolName)}</span>
                  <span className={styles.toolNameRaw}>({call.toolName})</span>
                </div>

                {/* Arguments Section */}
                <div className={styles.section}>
                  <button
                    className={styles.sectionHeader}
                    onClick={() => toggleSection(argsId)}
                  >
                    <span className={`${styles.chevron} ${argsExpanded ? styles.chevronExpanded : ''}`}>
                      ▶
                    </span>
                    <span className={styles.sectionTitle}>Arguments</span>
                    <button
                      className={styles.copyButton}
                      onClick={(e) => {
                        e.stopPropagation();
                        copyToClipboard(formattedArgs, argsId);
                      }}
                      title="Copy to clipboard"
                    >
                      {copiedId === argsId ? '✓' : '📋'}
                    </button>
                  </button>
                  {argsExpanded && (
                    <pre className={styles.jsonContent}>{formattedArgs}</pre>
                  )}
                </div>

                {/* Result Section */}
                <div className={styles.section}>
                  <button
                    className={styles.sectionHeader}
                    onClick={() => toggleSection(resultId)}
                  >
                    <span className={`${styles.chevron} ${resultExpanded ? styles.chevronExpanded : ''}`}>
                      ▶
                    </span>
                    <span className={styles.sectionTitle}>Result</span>
                    <button
                      className={styles.copyButton}
                      onClick={(e) => {
                        e.stopPropagation();
                        copyToClipboard(formattedResult, resultId);
                      }}
                      title="Copy to clipboard"
                    >
                      {copiedId === resultId ? '✓' : '📋'}
                    </button>
                  </button>
                  {resultExpanded && (
                    <pre className={styles.jsonContent}>{formattedResult}</pre>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};

export default ToolDetailsModal;
