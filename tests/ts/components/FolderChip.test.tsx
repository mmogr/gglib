/**
 * The chat's folder in the composer's margin, in its three states: none
 * chosen, a chip named Folder that chooses one; one chosen, its name, the
 * whole path on hover, Clear on the chip and Change beside it; and a send
 * refused for it, the chip a warning whose title says why. Change opens the
 * native dialog on desktop and a field for the path on web.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { FolderChip, folderName, folderOffer, folderTitle } from '../../../src/components/ChatMessagesPanel/components/FolderChip';
import { FOLDER_GONE, type ChatFolder } from '../../../src/hooks/useChatFolder';

function folder(over: Partial<ChatFolder> = {}): ChatFolder {
  return {
    shown: true,
    path: null,
    error: null,
    native: false,
    sentTo: null,
    unread: false,
    pick: vi.fn(async () => {}),
    choose: vi.fn(),
    clear: vi.fn(),
    forSend: () => undefined,
    ...over,
  };
}

describe('FolderChip', () => {
  it('with no folder, is a button named Folder that offers one', async () => {
    const user = userEvent.setup();
    const held = folder({ native: true });
    render(<FolderChip folder={held} />);
    const chip = screen.getByRole('button', { name: 'Folder' });
    expect(chip).toHaveAttribute('title', folderOffer(held));
    expect(chip.getAttribute('title')).toContain('on this machine');
    await user.click(chip);
    expect(held.pick).toHaveBeenCalledTimes(1);
  });

  it('with a folder, shows its name, the whole path on hover, and offers Change and Clear', async () => {
    const user = userEvent.setup();
    const held = folder({ path: '/home/m/src/gglib', native: true });
    render(<FolderChip folder={held} />);
    const name = screen.getByText('gglib');
    expect(name.closest('[title]')).toHaveAttribute('title', folderTitle('/home/m/src/gglib', held));

    await user.click(screen.getByRole('button', { name: 'Change' }));
    expect(held.pick).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: 'Clear the folder' }));
    expect(held.clear).toHaveBeenCalledTimes(1);
  });

  it('where a send was refused for the folder, is a warning that says why', () => {
    render(<FolderChip folder={folder({ path: '/home/m/gone', error: FOLDER_GONE })} />);
    const name = screen.getByText('gone');
    expect(name.closest('[title]')).toHaveAttribute('title', `/home/m/gone: ${FOLDER_GONE}`);
    expect(screen.getByText(`: ${FOLDER_GONE}`)).toHaveClass('sr-only');
    // Both ways out are still offered.
    expect(screen.getByRole('button', { name: 'Change' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Clear the folder' })).toBeInTheDocument();
  });

  it('on web, takes the path typed: Enter uses it, Escape leaves it as it was', async () => {
    const user = userEvent.setup();
    const held = folder();
    render(<FolderChip folder={held} />);

    await user.click(screen.getByRole('button', { name: 'Folder' }));
    await user.type(screen.getByRole('textbox', { name: 'Folder path' }), '/home/m/notes{Enter}');
    expect(held.choose).toHaveBeenCalledWith('/home/m/notes');
    expect(screen.queryByRole('textbox', { name: 'Folder path' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Folder' }));
    await user.type(screen.getByRole('textbox', { name: 'Folder path' }), '/elsewhere{Escape}');
    expect(held.choose).toHaveBeenCalledTimes(1);
    expect(held.pick).not.toHaveBeenCalled();
  });

  it('on web, offers a folder on the machine gglib runs on', () => {
    render(<FolderChip folder={folder()} />);
    const title = screen.getByRole('button', { name: 'Folder' }).getAttribute('title');
    expect(title).toContain('on the machine gglib runs on');
  });

  it("with the paired machine's model, says what is read is sent there", () => {
    const paired = { native: true, sentTo: 'desk', unread: false };
    expect(folderOffer(paired)).toMatch(/ What is read is sent to desk's model\.$/);
    expect(folderTitle('/home/m/src', paired)).toMatch(/read \/home\/m\/src, read-only\..* What is read is sent to desk's model\.$/);
    expect(folderTitle('/home/m/src', { sentTo: null, unread: false })).not.toContain('sent to');
  });

  it('for a model that calls no tools, says the folder is not read', () => {
    render(<FolderChip folder={folder({ path: '/home/m/src', unread: true })} />);
    const title = screen.getByText('src').closest('[title]')?.getAttribute('title');
    expect(title).toBe('This model calls no tools, so /home/m/src is not read. Clear forgets it.');
  });

  it('names a folder by its last name', () => {
    expect(folderName('/home/m/src/gglib/')).toBe('gglib');
    expect(folderName('C:\\Users\\m\\notes')).toBe('notes');
    expect(folderName('/')).toBe('/');
  });
});
