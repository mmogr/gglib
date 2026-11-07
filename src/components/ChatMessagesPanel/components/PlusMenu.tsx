import { FC, useContext, useState } from 'react';
import { Plus } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Menu, type MenuItem } from '../../ui/Menu';
import { getToolRegistry } from '../../../services/tools';
import type { ChatFolder } from '../../../hooks/useChatFolder';
import type { DrawSwitch } from '../../../hooks/useDrawSwitch';
import { ImageInputContext, useAddImage } from './ComposerImages';
import { AgentLimitsSheet } from './AgentLimitsSheet';
import { changeFolder } from './FolderChip';
import { drawItem, folderItem, imageItem, toolsItem, type ToolsChecklist } from './composerMenus';

interface PlusMenuProps {
  /** Whether a server answers the chat: no image is offered while none does. */
  connected: boolean;
  /** A far chat: no folder here. */
  far: boolean;
  /** This machine's model, by its registry id, for the agent limits' effort row. */
  modelId?: number;
  supportsToolCalls?: boolean | null;
  toolFormat?: string | null;
  folder?: ChatFolder;
  /** Open the field a folder's path is typed in (web). */
  onTypeFolder: (on: boolean) => void;
  draw?: DrawSwitch;
}

/** The registry, read fresh: the checklist is drawn again after every switch. */
function useToolsChecklist(): ToolsChecklist & { reread: () => void } {
  const [, setVersion] = useState(0);
  const registry = getToolRegistry();
  const reread = () => setVersion((v) => v + 1);
  return {
    tools: registry.getDefinitions(),
    isEnabled: (name) => registry.isEnabled(name),
    set: (names, on) => {
      names.forEach((name) => (on ? registry.enable(name) : registry.disable(name)));
      reread();
    },
    reread,
  };
}

/**
 * The composer's `[+]`: Add image…, Use a folder…, Draw a picture and
 * Tools ▸, each disabled with its reason where it cannot be used now
 * (`composerMenus`). The tools are the registry's, read again each time the
 * menu opens, so a tool registered since (built-in or MCP) is listed
 * without opening anything else. Tools ▸ ends with "Agent limits…", which
 * opens `AgentLimitsSheet`.
 */
export const PlusMenu: FC<PlusMenuProps> = ({
  connected,
  far,
  modelId,
  supportsToolCalls,
  toolFormat,
  folder,
  onTypeFolder,
  draw,
}) => {
  const imageInput = useContext(ImageInputContext);
  const addImage = useAddImage();
  const checklist = useToolsChecklist();
  const [limitsOpen, setLimitsOpen] = useState(false);

  const items = [
    imageItem(imageInput, connected, addImage),
    folderItem(folder, far, () => folder && changeFolder(folder, onTypeFolder)),
    drawItem(draw),
    toolsItem(checklist, supportsToolCalls, toolFormat, () => setLimitsOpen(true)),
  ].filter((item): item is MenuItem => item !== null);

  return (
    <>
      <Menu
        label={<Icon icon={Plus} size={16} />}
        name="Add to the message"
        iconOnly
        align="start"
        side="above"
        items={items}
        onOpen={checklist.reread}
      />
      <AgentLimitsSheet open={limitsOpen} onClose={() => setLimitsOpen(false)} modelId={modelId} />
    </>
  );
};
