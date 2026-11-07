import { FC, useEffect, useRef, useState } from 'react';
import { Modal } from '../../ui/Modal';
import { AgentLimitsSection } from '../../AgentLimitsSection';
import { getTransport } from '../../../services/transport';
import type { TemplateSupport } from '../../../types';

interface AgentLimitsSheetProps {
  open: boolean;
  onClose: () => void;
  /**
   * This machine's model the chat is with, by its registry id. Absent for a
   * chat on another machine's model, which has no id here: the reasoning
   * effort row then says it applies where a template reads it.
   */
  modelId?: number;
}

/**
 * The agent limits, in a small sheet opened from the composer's `[+]` ›
 * Tools ▸ › "Agent limits…". An interim home: Settings › Tools › Agent
 * limits takes it (S1 `feat(gui)/settings-place`, S2
 * `feat(gui)/settings-tools`).
 *
 * Whether the model's template reads a reasoning effort is asked once, the
 * first time the sheet opens for that model, never at mount, and kept with
 * the id it was asked of, so it never describes another model.
 */
export const AgentLimitsSheet: FC<AgentLimitsSheetProps> = ({ open, onClose, modelId }) => {
  const [effort, setEffort] = useState<{ modelId: number; support: TemplateSupport } | null>(null);
  const asked = useRef<number | null>(null);
  useEffect(() => {
    if (!open || modelId === undefined || asked.current === modelId) return;
    asked.current = modelId;
    getTransport().getModelDetail(modelId)
      .then((detail) => detail && setEffort({ modelId, support: detail.reasoningEffortSupport }))
      .catch(() => {
        // Not known: the row keeps its control, as it does for no model.
      });
  }, [open, modelId]);
  const effortSupport = effort !== null && effort.modelId === modelId ? effort.support : undefined;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title="Agent limits"
      description="Apply to every chat this device sends."
      size="sm"
      bodyClassName="p-0"
    >
      {open && <AgentLimitsSection effortSupport={effortSupport} />}
    </Modal>
  );
};
