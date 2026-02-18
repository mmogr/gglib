/**
 * ToolsPopover component - displays registered tools with enable/disable toggles.
 * Self-contained component with trigger button included.
 */

import React, { useRef, useState, useEffect, useCallback } from 'react';
import { Calculator, Clock3, FileText, Search, SunMedium, Wrench } from 'lucide-react';
import { useClickOutside } from '../../hooks/useClickOutside';
import { getToolRegistry, type ToolDefinition } from '../../services/tools';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';

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
const getToolIcon = (name: string) => {
  if (name.includes('time') || name.includes('date')) return Clock3;
  if (name.includes('file') || name.includes('read') || name.includes('write')) return FileText;
  if (name.includes('search') || name.includes('web')) return Search;
  if (name.includes('calc') || name.includes('math')) return Calculator;
  if (name.includes('weather')) return SunMedium;
  return Wrench;
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
    <div className="relative inline-block" ref={containerRef}>
      <Button
        variant="ghost"
        size="sm"
        className="relative"
        onClick={() => setIsOpen(!isOpen)}
        title="Tools"
        iconOnly
      >
        <Icon icon={Wrench} size={14} />
        {enabledTools.size > 0 && (
          <span className="absolute -top-1 -right-1 min-w-[14px] h-[14px] px-1 text-[9px] font-semibold leading-[14px] text-center text-white bg-accent rounded-[7px]">{enabledTools.size}</span>
        )}
      </Button>

      {isOpen && (
        <div className="absolute top-full right-0 mt-1 bg-surface border border-border rounded-lg shadow-[0_4px_16px_rgba(0,0,0,0.3)] min-w-[300px] max-w-[400px] z-popover overflow-hidden" ref={popoverRef}>
          <div className="flex items-center justify-between px-[14px] py-[10px] border-b border-border bg-surface-elevated">
            <span className="text-[13px] font-semibold text-text-primary">
              <Icon icon={Wrench} size={14} />
              <span className="ml-1.5">Tools</span>
            </span>
            <span className="text-[11px] text-text-secondary bg-surface px-2 py-[2px] rounded-[10px]">
              {enabledTools.size}/{tools.length} active
            </span>
          </div>

          {tools.length === 0 ? (
            <div className="px-[14px] py-6 text-center">
              <p className="m-0 text-text-secondary text-[13px]">No tools registered</p>
              <p className="mt-2 text-[11px] text-text-muted">
                Tools can be added via the tool registry API.
              </p>
            </div>
          ) : (
            <>
              {/* Toggle all row */}
              <div className="px-[14px] py-2 border-b border-border bg-surface-elevated">
                <label className="flex items-center gap-2 cursor-pointer text-[12px] text-text-secondary hover:text-text-primary">
                  <input
                    type="checkbox"
                    checked={allEnabled}
                    ref={(el) => {
                      if (el) el.indeterminate = !allEnabled && !noneEnabled;
                    }}
                    onChange={handleToggleAll}
                    className="mt-[2px] accent-accent cursor-pointer"
                  />
                  <span>{allEnabled ? 'Disable all' : 'Enable all'}</span>
                </label>
              </div>

              {/* Tool list */}
              <div className="max-h-[280px] overflow-y-auto scrollbar-thin">
                {tools.map((tool) => {
                  const name = tool.function.name;
                  const enabled = enabledTools.has(name);
                  const icon = getToolIcon(name);
                  const displayName = formatToolName(name);
                  const description = tool.function.description || 'No description';

                  return (
                    <div
                      key={name}
                      className={cn(
                        'px-[14px] py-[10px] border-b border-border-subtle last:border-b-0 hover:bg-surface-hover transition-colors duration-150',
                        !enabled && 'opacity-50',
                      )}
                    >
                      <label className="flex items-start gap-[10px] cursor-pointer">
                        <input
                          type="checkbox"
                          checked={enabled}
                          onChange={(e) => handleToggleTool(name, e.target.checked)}
                          className="mt-[2px] accent-accent cursor-pointer"
                        />
                        <span className="text-[18px] -mt-[1px]" aria-hidden>
                          <Icon icon={icon} size={14} />
                        </span>
                        <div className="flex-1 min-w-0">
                          <span className="block text-[13px] font-medium text-text-primary mb-[2px]">{displayName}</span>
                          <span className="text-[11px] text-text-secondary leading-[1.4] line-clamp-2">{description}</span>
                        </div>
                      </label>
                    </div>
                  );
                })}
              </div>
            </>
          )}

          <div className="px-[14px] py-2 border-t border-border bg-surface-elevated">
            <span className="text-[10px] text-text-muted italic">
              Enabled tools are sent to the model for function calling.
            </span>
          </div>
        </div>
      )}
    </div>
  );
};

export default ToolsPopover;
