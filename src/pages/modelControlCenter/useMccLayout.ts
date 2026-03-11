import { usePanelResize, UsePanelResizeResult } from '../../hooks/usePanelResize';

export type UseMccLayoutResult = UsePanelResizeResult;

export function useMccLayout(): UseMccLayoutResult {
  return usePanelResize({ initial: 45, min: 25, max: 60 });
}
