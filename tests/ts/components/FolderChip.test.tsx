/**
 * The chat's folder chip in the composer, drawn only once a folder is set:
 * its name, the whole path on hover, a click to choose another and × to
 * clear it; a send refused for it makes it a warning whose title says why.
 * Choosing opens the native dialog on desktop and a field for the path on
 * web, which `[+]` › Use a folder… opens too (`ChatPlaceFolder.test.tsx`).
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { useState } from 'react';
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

/** The chip with the typing state its composer holds. */
function Chip({ held, typing = false }: { held: ChatFolder; typing?: boolean }) {
  const [open, setOpen] = useState(typing);
  return <FolderChip folder={held} typing={open} setTyping={setOpen} />;
}

describe('FolderChip', () => {
  it('draws nothing with no folder set', () => {
    const { container } = render(<Chip held={folder({ native: true })} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('with a folder, shows its name, the whole path on hover; a click chooses another and × clears it', async () => {
    const user = userEvent.setup();
    const held = folder({ path: '/home/m/src/gglib', native: true });
    render(<Chip held={held} />);
    const chip = screen.getByRole('button', { name: 'gglib' });
    expect(chip).toHaveAttribute('title', folderTitle('/home/m/src/gglib', held));
    // Inside the composer's form: never a submit. Chip text is 12 px.
    expect(chip).toHaveAttribute('type', 'button');
    expect(chip).toHaveClass('text-xs');
    expect(chip).not.toHaveClass('text-sm');

    await user.click(chip);
    expect(held.pick).toHaveBeenCalledTimes(1);
    const clear = screen.getByRole('button', { name: 'Clear the folder' });
    expect(clear).toHaveAttribute('type', 'button');
    await user.click(clear);
    expect(held.clear).toHaveBeenCalledTimes(1);
  });

  it('where a send was refused for the folder, is a warning that says why, and keeps both ways out', () => {
    render(<Chip held={folder({ path: '/home/m/gone', error: FOLDER_GONE })} />);
    const chip = screen.getByRole('button', { name: `gone: ${FOLDER_GONE}` });
    expect(chip).toHaveAttribute('title', `/home/m/gone: ${FOLDER_GONE}`);
    expect(screen.getByText(`: ${FOLDER_GONE}`)).toHaveClass('sr-only');
    expect(chip.parentElement).toHaveClass('text-warning');
    expect(screen.getByRole('button', { name: 'Clear the folder' })).toBeInTheDocument();
  });

  it('on web, takes the path typed: Enter uses it, Escape leaves it as it was', async () => {
    const user = userEvent.setup();
    const held = folder({ path: '/home/m/src' });
    render(<Chip held={held} />);

    await user.click(screen.getByRole('button', { name: 'src' }));
    const field = screen.getByRole('textbox', { name: 'Folder path' });
    expect(field).toHaveValue('/home/m/src');
    await user.clear(field);
    await user.type(field, '/home/m/notes{Enter}');
    expect(held.choose).toHaveBeenCalledWith('/home/m/notes');
    expect(screen.queryByRole('textbox', { name: 'Folder path' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'src' }));
    await user.type(screen.getByRole('textbox', { name: 'Folder path' }), '/elsewhere{Escape}');
    expect(held.choose).toHaveBeenCalledTimes(1);
    expect(held.pick).not.toHaveBeenCalled();
  });

  it('opens the field empty when the menu opens it with no folder set', () => {
    render(<Chip held={folder()} typing />);
    expect(screen.getByRole('textbox', { name: 'Folder path' })).toHaveValue('');
  });

  it('offers a folder on this machine on desktop, and on the machine gglib runs on, on web', () => {
    expect(folderOffer({ native: true, sentTo: null })).toContain('on this machine');
    expect(folderOffer({ native: false, sentTo: null })).toContain('on the machine gglib runs on');
  });

  it("with the paired machine's model, says what is read is sent there", () => {
    const paired = { native: true, sentTo: 'desk', unread: false };
    expect(folderOffer(paired)).toMatch(/ What is read is sent to desk's model\.$/);
    expect(folderTitle('/home/m/src', paired)).toMatch(/read \/home\/m\/src, read-only\..* What is read is sent to desk's model\.$/);
    expect(folderTitle('/home/m/src', { sentTo: null, unread: false })).not.toContain('sent to');
  });

  it('for a model that calls no tools, says the folder is not read', () => {
    render(<Chip held={folder({ path: '/home/m/src', unread: true })} />);
    expect(screen.getByRole('button', { name: 'src' })).toHaveAttribute(
      'title',
      'This model calls no tools, so /home/m/src is not read. × forgets it.',
    );
  });

  it('names a folder by its last name', () => {
    expect(folderName('/home/m/src/gglib/')).toBe('gglib');
    expect(folderName('C:\\Users\\m\\notes')).toBe('notes');
    expect(folderName('/')).toBe('/');
  });
});
