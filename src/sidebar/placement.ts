/**
 * Where a panel a sidebar line opens is drawn: beside the line when the
 * sidebar is open, and across the window's width under the top strip when
 * the sidebar is a drawer, since a drawer is narrower than the panel. Beside
 * the line, the panel hangs from the line's top, so its height is capped at
 * what is left of the window below the lines above it, and it scrolls.
 *
 * @module sidebar/placement
 */
export const POPOVER_BESIDE =
  'fixed inset-x-sm top-16 max-h-[calc(100vh-5rem)] wide:absolute wide:inset-x-auto wide:left-full wide:top-0 wide:ml-sm wide:max-h-[calc(100vh-14rem)]';
