import { FC, useContext, useState } from 'react';
import { Plus } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Menu, type MenuItem } from '../../ui/Menu';
import { getToolRegistry } from '../../../services/tools';
import type { ChatFolder } from '../../../hooks/useChatFolder';
import type { DrawSwitch } from '../../../hooks/useDrawSwitch';
import type { ChatTools } from '../../../hooks/useChatTools';
import { ImageInputContext, useAddImage } from './ComposerImages';
import { changeFolder } from './FolderChip';
import { drawItem, folderItem, imageItem, toolsItem, type ListedTool } from './composerMenus';

interface PlusMenuProps {
  /** Whether a server answers the chat: no image is offered while none does. */
  connected: boolean;
  /** A far chat: no folder here. */
  far: boolean;
  supportsToolCalls?: boolean | null;
  toolFormat?: string | null;
  folder?: ChatFolder;
  /** Open the field a folder's path is typed in (web). */
  onTypeFolder: (on: boolean) => void;
  draw?: DrawSwitch;
  /** The chat's saved tools; none on a far chat. */
  tools?: ChatTools;
}

/** The registry's tools, read fresh: the list is read again each time the menu opens. */
function useListedTools(): { listed: ListedTool[]; reread: () => void } {
  const [, setVersion] = useState(0);
  const registry = getToolRegistry();
  return {
    listed: registry.getDefinitions().map((definition) => ({ definition, name: registry.getBackendName(definition.function.name) })),
    reread: () => setVersion((v) => v + 1),
  };
}

/**
 * The composer's `[+]`: Add image…, Use a folder…, Draw a picture and
 * Tools ▸, each disabled with its reason where it cannot be used now
 * (`composerMenus`). The tools are the registry's, read again each time the
 * menu opens, so a tool registered since (built-in or MCP) is listed
 * without opening anything else; which of them are ticked is the chat's
 * saved choice (`tools`).
 */
export const PlusMenu: FC<PlusMenuProps> = ({
  connected,
  far,
  supportsToolCalls,
  toolFormat,
  folder,
  onTypeFolder,
  draw,
  tools,
}) => {
  const imageInput = useContext(ImageInputContext);
  const addImage = useAddImage();
  const { listed, reread } = useListedTools();

  const items = [
    imageItem(imageInput, connected, addImage),
    folderItem(folder, far, () => folder && changeFolder(folder, onTypeFolder)),
    drawItem(draw),
    toolsItem(listed, tools, supportsToolCalls, toolFormat),
  ].filter((item): item is MenuItem => item !== null);

  return (
    <Menu
      label={<Icon icon={Plus} size={16} />}
      name="Add to the message"
      iconOnly
      align="start"
      side="above"
      items={items}
      onOpen={reread}
    />
  );
};
