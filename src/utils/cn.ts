import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Combines `clsx` for conditional class composition with `tailwind-merge`
 * for deduplication of conflicting Tailwind utility classes.
 *
 * @example
 * cn("px-4 py-2", isActive && "bg-primary", className)
 * cn("text-sm font-medium", { "opacity-50": disabled })
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
