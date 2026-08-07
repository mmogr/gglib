import { ReactNode } from "react";
import { cn } from "../../utils/cn";

export interface TabItem<T extends string = string> {
  id: T;
  label: string;
  icon?: ReactNode;
}

export interface TabsProps<T extends string = string> {
  tabs: TabItem<T>[];
  activeId: T;
  onChange: (id: T) => void;
  /** Accessible name for the tablist — required so tab bars are never anonymous. */
  "aria-label": string;
  size?: "sm" | "md";
  /** Stretch tabs to share the row width (sidebar style) instead of hugging content. */
  fill?: boolean;
  /** Hairline under the whole row. Disable when the container draws its own. */
  divider?: boolean;
  /** Content rendered on the right side of the row (e.g. action buttons). */
  rightContent?: ReactNode;
  className?: string;
}

/**
 * The one tab treatment: inactive tabs are muted text, the active tab is
 * plain text weight with a 2px primary underline bar. Accent color is not
 * spent on active-tab text.
 */
export function Tabs<T extends string = string>({
  tabs,
  activeId,
  onChange,
  size = "md",
  fill = false,
  divider = true,
  rightContent,
  className,
  "aria-label": ariaLabel,
}: TabsProps<T>) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-md",
        divider && "border-b border-border-light",
        className,
      )}
    >
      <div role="tablist" aria-label={ariaLabel} className={cn("flex gap-xs", fill && "flex-1")}>
        {tabs.map((tab) => {
          const active = tab.id === activeId;
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={active}
              onClick={() => onChange(tab.id)}
              className={cn(
                "relative flex items-center justify-center gap-xs bg-transparent cursor-pointer font-medium whitespace-nowrap transition-colors",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-inset rounded-t-base",
                size === "sm" ? "px-sm py-xs text-xs" : "px-md py-sm text-sm",
                fill && "flex-1",
                active ? "text-text" : "text-text-muted hover:text-text",
                // Active indicator: a bar, not a border — always present so
                // activation never shifts layout.
                "after:absolute after:inset-x-sm after:bottom-0 after:h-0.5 after:rounded-full after:transition-colors",
                active ? "after:bg-primary" : "after:bg-transparent",
              )}
            >
              {tab.icon && (
                <span className="inline-flex items-center [&>svg]:w-4 [&>svg]:h-4">{tab.icon}</span>
              )}
              <span className="overflow-hidden text-ellipsis">{tab.label}</span>
            </button>
          );
        })}
      </div>
      {rightContent && <div className="flex items-center gap-sm shrink-0">{rightContent}</div>}
    </div>
  );
}
