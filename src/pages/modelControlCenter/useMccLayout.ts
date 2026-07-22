import { usePanelResize, UsePanelResizeResult } from '../../hooks/usePanelResize';

export type UseMccLayoutResult = UsePanelResizeResult;

export function useMccLayout(): UseMccLayoutResult {
  // 32% suits a list of one-line rows; 45% left most of the panel empty.
  return usePanelResize({
    initial: 32,
    min: 24,
    max: 50,
    storageKey: 'gglib.mcc.split',
  });
}
