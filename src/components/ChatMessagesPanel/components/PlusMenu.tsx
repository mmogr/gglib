import { FC, useCallback, useContext, useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Menu, type MenuItem } from '../../ui/Menu';
import { getToolRegistry } from '../../../services/tools';
import { getTransport } from '../../../services/transport';
import { formatError } from '../../../utils/errors';
import type { ChatFolder } from '../../../hooks/useChatFolder';
import type { DrawSwitch } from '../../../hooks/useDrawSwitch';
import type { ChatTools } from '../../../hooks/useChatTools';
import { ImageInputContext, useAddImage } from './ComposerImages';
import { changeFolder } from './FolderChip';
import { ASKING, drawItem, folderItem, imageItem, toolRows, toolsItem, type EnabledServer, type ListedTool } from './composerMenus';

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

/** The checklist's rows, or why there are none yet: the registry's tools and the enabled servers, read again each time the menu opens. */
function useListedTools(): { listed: ListedTool[] | string; reread: () => void } {
  const [servers, setServers] = useState<EnabledServer[] | string>(ASKING);
  const reread = useCallback(() => {
    Promise.resolve()
      .then(() => getTransport().listMcpServers())
      .then((infos) => setServers(infos.filter((info) => info.server.enabled).map(({ server }) => ({ id: server.id, name: server.name }))))
      .catch((error: unknown) => setServers(`Couldn't read the tool servers: ${formatError(error)}`));
  }, []);
  useEffect(reread, [reread]);
  if (typeof servers === 'string') return { listed: servers, reread };
  const registry = getToolRegistry();
  const registered = registry.getDefinitions().map((definition) => {
    const key = definition.function.name;
    const source = registry.getSource(key);
    const server = source?.startsWith('mcp:') ? Number(source.slice(4)) : null;
    return { definition, name: registry.getBackendName(key), server };
  });
  return { listed: toolRows(registered, servers), reread };
}

/**
 * The composer's `[+]`: Add image…, Use a folder…, Draw a picture and
 * Tools ▸, each disabled with its reason where it cannot be used now
 * (`composerMenus`). The tools are the registry's and each enabled tool
 * server, read again each time the menu opens, so a tool registered since
 * (built-in or MCP) is listed without opening anything else, and a server
 * not started is one row for all its tools; which of them are ticked is the
 * chat's saved choice (`tools`). Nothing here starts a server.
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
