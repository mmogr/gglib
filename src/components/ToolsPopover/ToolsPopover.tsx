/**
 * ToolsPopover component - displays registered tools with enable/disable toggles.
 * Self-contained component with trigger button included.
 */

import React, { useRef, useState, useEffect, useCallback } from 'react';
import { useClickOutside } from '../../hooks/useClickOutside';
import { getToolRegistry, type ToolDefinition } from '../../services/tools';
import styles from './ToolsPopover.module.css';

/**
 * Get a human-readable name from a tool function name.
 * e.g., "get_current_time" -> "Get Current Time"
 */
const formatToolName = (name: string): string => {
  return name
    .split('_')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
};

/**
 * Get an icon for a tool based on its name.
 */
const getToolIcon = (name: string): string => {
  if (name.includes('time') || name.includes('date')) return '🕐';
  if (name.includes('file') || name.includes('read') || name.includes('write')) return '📄';
  if (name.includes('search') || name.includes('web')) return '🔍';
  if (name.includes('calc') || name.includes('math')) return '🔢';
  if (name.includes('weather')) return '🌤️';
  return '🔧';
};

export const ToolsPopover: React.FC = () => {
  const containerRef = useRef<HTMLDivElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [tools, setTools] = useState<ToolDefinition[]>([]);
  const [enabledTools, setEnabledTools] = useState<Set<string>>(new Set());

  // Close when clicking outside
  useClickOutside(popoverRef, () => setIsOpen(false), isOpen);

  // Load tools and their enabled state
  const refreshTools = useCallback(() => {
    const registry = getToolRegistry();
    const definitions = registry.getDefinitions();
    setTools(definitions);
    
    // Build enabled set from registry
    const enabled = new Set<string>();
    definitions.forEach((t) => {
      if (registry.isEnabled(t.function.name)) {
        enabled.add(t.function.name);
      }
    });
    setEnabledTools(enabled);
  }, []);

  // Refresh tools on open
  useEffect(() => {
    if (isOpen) {
      refreshTools();
    }
  }, [isOpen, refreshTools]);

  // Initial load
  useEffect(() => {
    refreshTools();
  }, [refreshTools]);

  const handleToggleTool = (toolName: string, enabled: boolean) => {
    const registry = getToolRegistry();
    if (enabled) {
      registry.enable(toolName);
    } else {
      registry.disable(toolName);
    }
    // Update local state
    setEnabledTools((prev) => {
      const next = new Set(prev);
      if (enabled) {
        next.add(toolName);
      } else {
        next.delete(toolName);
      }
      return next;
    });
  };

  const allEnabled = tools.length > 0 && tools.every((t) => enabledTools.has(t.function.name));
  const noneEnabled = tools.every((t) => !enabledTools.has(t.function.name));

  const handleToggleAll = () => {
    const registry = getToolRegistry();
    if (allEnabled) {
      // Disable all
      tools.forEach((t) => {
        registry.disable(t.function.name);
      });
      setEnabledTools(new Set());
    } else {
      // Enable all
      const allNames = new Set<string>();
      tools.forEach((t) => {
        registry.enable(t.function.name);
        allNames.add(t.function.name);
      });
      setEnabledTools(allNames);
    }
  };

  return (
    <div className={styles.container} ref={containerRef}>
      <button
        className={`icon-btn icon-btn-sm ${styles.triggerButton}`}
        onClick={() => setIsOpen(!isOpen)}
        title="Tools"
      >
        🔧
        {enabledTools.size > 0 && (
          <span className={styles.badge}>{enabledTools.size}</span>
        )}
      </button>

      {isOpen && (
        <div className={styles.popover} ref={popoverRef}>
          <div className={styles.header}>
            <span className={styles.title}>🔧 Tools</span>
            <span className={styles.count}>
              {enabledTools.size}/{tools.length} active
            </span>
          </div>

          {tools.length === 0 ? (
            <div className={styles.emptyState}>
              <p>No tools registered</p>
              <p className={styles.emptyHint}>
                Tools can be added via the tool registry API.
              </p>
            </div>
          ) : (
            <>
              {/* Toggle all row */}
              <div className={styles.toggleAllRow}>
                <label className={styles.toggleAllLabel}>
                  <input
                    type="checkbox"
                    checked={allEnabled}
                    ref={(el) => {
                      if (el) el.indeterminate = !allEnabled && !noneEnabled;
                    }}
                    onChange={handleToggleAll}
                    className={styles.checkbox}
                  />
                  <span>{allEnabled ? 'Disable all' : 'Enable all'}</span>
                </label>
              </div>

              {/* Tool list */}
              <div className={styles.content}>
                {tools.map((tool) => {
                  const name = tool.function.name;
                  const enabled = enabledTools.has(name);
                  const icon = getToolIcon(name);
                  const displayName = formatToolName(name);
                  const description = tool.function.description || 'No description';

                  return (
                    <div
                      key={name}
                      className={`${styles.toolItem} ${!enabled ? styles.toolItemDisabled : ''}`}
                    >
                      <label className={styles.toolLabel}>
                        <input
                          type="checkbox"
                          checked={enabled}
                          onChange={(e) => handleToggleTool(name, e.target.checked)}
                          className={styles.checkbox}
                        />
                        <span className={styles.toolIcon}>{icon}</span>
                        <div className={styles.toolInfo}>
                          <span className={styles.toolName}>{displayName}</span>
                          <span className={styles.toolDescription}>{description}</span>
                        </div>
                      </label>
                    </div>
                  );
                })}
              </div>
            </>
          )}

          <div className={styles.footer}>
            <span className={styles.footerHint}>
              Enabled tools are sent to the model for function calling.
            </span>
          </div>
        </div>
      )}
    </div>
  );
};

export default ToolsPopover;
