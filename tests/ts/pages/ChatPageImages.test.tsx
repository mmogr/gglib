/**
 * Images on the chat page: attached by paste, drop or the attach button,
 * each shown in the composer with what it costs, or a refusal said in a
 * toast; offered only where the model reads images, in an edit's composer
 * too; a saved turn's images read
 * from the chat's store with the page's credential, enlarged on a click,
 * and let go when they leave; and the images of an unsent message carried
 * over a model switch without being uploaded again.
 *
 * jsdom draws no image and makes no `blob:` URL, so `URL.createObjectURL`
 * is stubbed; a paste and a drop are dispatched as events, not done by hand.
 * assistant-ui cancels a paste or a drop it takes before it awaits anything,
 * so whether `fireEvent` returns false says at once whether it was taken.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { ReactNode } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ChatMessage } from '../../../src/services/transport';
import { chatTransport, conversation, wrapper as pageWrapper, type ChatFixture } from './chatPageHarness';
import { pngFile } from '../fixtures/fakeImageStore';
import { guiModel } from '../fixtures/model';
import type { AttachmentUpload } from '../../../src/types/generated/AttachmentUpload';
import type { ModelChoice } from '../../../src/components/ChatMessagesPanel';
import type { ChatDraft } from '../../../src/types/messages';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { useToastContext } from '../../../src/contexts/ToastContext';
import { TransportError } from '../../../src/services/transport/errors';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};
const wrapper = ({ children }: { children: ReactNode }) =>
  pageWrapper({ children: <><ToastProbe />{children}</> });

const SHOT = 'b'.repeat(64);

let fixture: ChatFixture;
let urls: number;
const uploadAttachment = vi.fn(async (_source: string, image: Blob): Promise<AttachmentUpload> => ({
  id: SHOT, mime: image.type, width: 800, height: 600, image_tokens: 475,
}));
const fetchAttachmentBlob = vi.fn(async (_source: string, _id: string) => new Blob(['png'], { type: 'image/png' }));

function page(model: { imageInput: boolean; contextLength?: number | null; serverDefaults?: { contextLength: number } }, extra: Record<string, unknown> = {}) {
  transport.current = {
    ...chatTransport(fixture),
    getModel: vi.fn(async () => ({ quantization: 'Q8_0', contextLength: null, ...model })),
    uploadAttachment,
    fetchAttachmentBlob,
    ...extra,
  };
}

function renderLocal(props: { draft?: ChatDraft; onSwitchModel?: (c: ModelChoice, ctx: () => { conversationId: number | null; draft: ChatDraft }) => Promise<void> } = {}) {
  return render(
    <ChatPage modelName="Qwen3.8-27B" modelId={7} serverPort={4321} onClose={async () => {}} {...props} />,
    { wrapper },
  );
}

const attachButton = () => screen.getByRole('button', { name: 'Attach an image' });

/**
 * The composer's text box once the open conversation is drawn: the thread,
 * composer and all, remounts when the conversation is chosen and again once
 * its rows are read (the head alone is shown meanwhile), and a box found
 * before then is gone.
 */
async function composerBox(): Promise<HTMLElement> {
  await waitFor(() => expect(screen.getByRole('option', { name: /Screenshots/ })).toHaveAttribute('aria-selected', 'true'));
  await screen.findByRole('heading', { name: 'Screenshots' });
  const { getMessages } = transport.current as { getMessages: (id: number) => Promise<ChatMessage[]> };
  await waitFor(() => expect(getMessages).toHaveBeenCalledWith(1));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
  return screen.findByRole('textbox', { name: 'Message' });
}
const strip = () => screen.getByRole('group', { name: 'Attached images' });

beforeEach(() => {
  window.localStorage.clear();
  urls = 0;
  uploadAttachment.mockClear();
  fetchAttachmentBlob.mockReset();
  vi.spyOn(URL, 'createObjectURL').mockImplementation(() => `blob:shown-${++urls}`);
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
  fixture = { conversations: [conversation(1, 'Screenshots')], rows: { 1: [] }, runs: [], frames: {} };
});
afterEach(() => {
  vi.restoreAllMocks();
});

describe('ChatPage, images in the composer', () => {
  it('a paste uploads the image at once, and its tile says what it costs against the served context', async () => {
    page({ imageInput: true, contextLength: 8_192, serverDefaults: { contextLength: 32_768 } });
    renderLocal();
    const box = await composerBox();
    await waitFor(() => expect(attachButton()).toBeEnabled());
    const shot = pngFile(800, 600, 'shot.png');

    expect(fireEvent.paste(box, { clipboardData: { files: [shot] } })).toBe(false);

    await waitFor(() => expect(within(strip()).getByText('~475 tokens · 1% of context')).toBeInTheDocument());
    expect(uploadAttachment).toHaveBeenCalledWith('this', shot);
    expect(within(strip()).getByRole('img', { name: 'shot.png' })).toHaveAttribute('src', 'blob:shown-1');
  });

  it('once a reply says the context it was answered with, a tile\'s share is of that size, the one the ring reads', async () => {
    fixture.rows = {
      1: [
        { id: 11, conversation_id: 1, role: 'user', content: 'Hello.', created_at: '2026-10-04T09:00:00Z' },
        {
          id: 12,
          conversation_id: 1,
          role: 'assistant',
          content: 'Hi.',
          created_at: '2026-10-04T09:00:05Z',
          metadata: { promptTokens: 900, completionTokens: 50, contextSize: 16_384 },
        },
      ],
    };
    page({ imageInput: true, contextLength: 8_192, serverDefaults: { contextLength: 32_768 } });
    renderLocal();
    const box = await composerBox();
    await waitFor(() => expect(attachButton()).toBeEnabled());
    expect(screen.getByRole('button', { name: /^Context: / })).toHaveAttribute(
      'title',
      '950 of 16,384 tokens (6%) after the last finished reply.',
    );

    expect(fireEvent.paste(box, { clipboardData: { files: [pngFile(800, 600, 'shot.png')] } })).toBe(false);

    // 475 of 16,384 is 3%; of the catalogue's 32,768 it would be 1%.
    await waitFor(() => expect(within(strip()).getByText('~475 tokens · 3% of context')).toBeInTheDocument());
  });

  it('a drop and the attach button add images too, and Remove takes one away', async () => {
    const user = userEvent.setup();
    page({ imageInput: true, contextLength: 4_096 });
    renderLocal();
    const box = await composerBox();
    await waitFor(() => expect(attachButton()).toBeEnabled());

    expect(fireEvent.drop(box.closest('form')!, { dataTransfer: { files: [pngFile(800, 600, 'dropped.png')] } })).toBe(false);
    await waitFor(() => expect(within(strip()).getByRole('img', { name: 'dropped.png' })).toBeInTheDocument());

    await user.click(attachButton());
    const picker = document.body.querySelector<HTMLInputElement>('input[type=file]')!;
    expect(picker.accept).toBe('image/png,image/jpeg');
    fireEvent.change(picker, { target: { files: [pngFile(800, 600, 'picked.png', 1)] } });
    await waitFor(() => expect(within(strip()).getByRole('img', { name: 'picked.png' })).toBeInTheDocument());
    expect(within(strip()).getAllByText('~475 tokens · 12% of context')).toHaveLength(2);

    await user.click(within(strip()).getByRole('button', { name: 'Remove dropped.png' }));
    await waitFor(() => expect(within(strip()).queryByRole('img', { name: 'dropped.png' })).not.toBeInTheDocument());
    expect(within(strip()).getByRole('img', { name: 'picked.png' })).toBeInTheDocument();
  });

  it('an image the store refuses is said in a toast, by its code', async () => {
    const tooLarge = new TransportError('VALIDATION', 'too large', { status: 413, type: 'image_too_large' });
    page({ imageInput: true }, { uploadAttachment: vi.fn(async () => { throw tooLarge; }) });
    renderLocal();
    const box = await composerBox();
    await waitFor(() => expect(attachButton()).toBeEnabled());
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();

    expect(fireEvent.paste(box, { clipboardData: { files: [pngFile(800, 600)] } })).toBe(false);

    await waitFor(() =>
      expect(screen.getByTestId('toasts')).toHaveTextContent('An image is over the 8 MiB one image may be. Send a smaller one.'),
    );
  });

  it('a model that cannot read images is offered no attach, no drop and no paste, and is told why', async () => {
    page({ imageInput: false });
    renderLocal();
    const box = await composerBox();
    await waitFor(() => expect(attachButton()).toBeDisabled());
    expect(attachButton()).toHaveAttribute('title', 'This model cannot read images: it has no projector.');
    const form = box.closest('form')!;
    const delivered = vi.fn();
    form.addEventListener('drop', delivered);
    box.addEventListener('paste', delivered);

    expect(fireEvent.drop(form, { dataTransfer: { files: [pngFile(800, 600)] } })).toBe(true);
    expect(fireEvent.paste(box, { clipboardData: { files: [pngFile(800, 600)] } })).toBe(true);

    expect(delivered).toHaveBeenCalledTimes(2);
    await act(async () => {});
    expect(uploadAttachment).not.toHaveBeenCalled();
    expect(screen.queryByRole('group', { name: 'Attached images' })).not.toBeInTheDocument();
  });
});

describe('ChatPage, images in the thread', () => {
  const saved = (images: string[]): ChatMessage => ({
    id: 11, conversation_id: 1, role: 'user', content: 'What is this?', created_at: '2026-10-04T09:00:00Z',
    images: images.map((id) => ({ id, mime: 'image/png', width: 800, height: 600 })),
  });

  it('a reopened turn shows its images, read from the store with the credential, enlarged on a click, let go after', async () => {
    const user = userEvent.setup();
    fixture.rows = { 1: [saved([SHOT])] };
    page({ imageInput: true });
    const view = renderLocal();

    const shown = await screen.findByRole('img', { name: 'Image, 800 × 600' });
    expect(fetchAttachmentBlob).toHaveBeenCalledWith('this', SHOT);
    expect(shown).toHaveAttribute('src', 'blob:shown-1');

    await user.click(screen.getByRole('button', { name: 'Enlarge image, 800 × 600' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('img', { name: 'Image, 800 × 600' })).toHaveAttribute('src', 'blob:shown-1');

    view.unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:shown-1');
  });

  it('a turn\'s image says it is loading until its bytes land', async () => {
    const land: Array<(blob: Blob) => void> = [];
    fetchAttachmentBlob.mockImplementation(() => new Promise<Blob>((resolve) => land.push(resolve)));
    fixture.rows = { 1: [saved([SHOT])] };
    page({ imageInput: true });
    renderLocal();

    expect(await screen.findByRole('status', { name: 'Loading image' })).toBeInTheDocument();
    act(() => land.forEach((resolve) => resolve(new Blob(['png'], { type: 'image/png' }))));
    expect(await screen.findByRole('img', { name: 'Image, 800 × 600' })).toBeInTheDocument();
    expect(screen.queryByRole('status', { name: 'Loading image' })).not.toBeInTheDocument();
  });

  /** The open edit of the saved turn: its text box, and the composer around it. */
  async function editTurn(): Promise<{ editBox: HTMLElement; edit: HTMLElement }> {
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Edit message' }));
    const editBox = await screen.findByRole('textbox', { name: 'Edit message' });
    return { editBox, edit: editBox.closest('form')! };
  }

  it('a paste into an edit takes no image for a model that cannot read them', async () => {
    fixture.rows = { 1: [saved([])] };
    page({ imageInput: false });
    renderLocal();
    await waitFor(() => expect(attachButton()).toBeDisabled());
    const { editBox, edit } = await editTurn();
    const delivered = vi.fn();
    editBox.addEventListener('paste', delivered);

    expect(fireEvent.paste(editBox, { clipboardData: { files: [pngFile(800, 600)] } })).toBe(true);

    expect(delivered).toHaveBeenCalledTimes(1);
    await act(async () => {});
    expect(uploadAttachment).not.toHaveBeenCalled();
    expect(within(edit).queryByRole('group', { name: 'Attached images' })).not.toBeInTheDocument();
    expect(screen.queryByRole('group', { name: 'Attached images' })).not.toBeInTheDocument();
  });

  it('a paste into an edit takes an image for a model that reads them, its cost against the model\'s context', async () => {
    fixture.rows = { 1: [saved([])] };
    page({ imageInput: true, contextLength: 8_192 });
    renderLocal();
    await waitFor(() => expect(attachButton()).toBeEnabled());
    const { editBox, edit } = await editTurn();
    const shot = pngFile(800, 600, 'shot.png');

    expect(fireEvent.paste(editBox, { clipboardData: { files: [shot] } })).toBe(false);

    const tiles = () => within(edit).getByRole('group', { name: 'Attached images' });
    await waitFor(() => expect(within(tiles()).getByText('~475 tokens · 6% of context')).toBeInTheDocument());
    expect(uploadAttachment).toHaveBeenCalledWith('this', shot);
    expect(within(tiles()).getByRole('img', { name: 'shot.png' })).toBeInTheDocument();
  });
});

describe('ChatPage, images over a model switch', () => {
  it('hands up the unsent images with the text, and a page given them shows them without uploading again', async () => {
    act(() => ingestServerEvent({ type: 'running', modelId: '8', port: 5555, updatedAt: Date.now(), modelName: 'llama-3.2-3b' }));
    const user = userEvent.setup();
    const onSwitchModel = vi.fn(async (_c: ModelChoice, _ctx: () => { conversationId: number | null; draft: ChatDraft }) => {});
    page({ imageInput: true }, {
      listModels: vi.fn(async () => [guiModel({ id: 7, name: 'Qwen3.8-27B' }), guiModel({ id: 8, name: 'llama-3.2-3b' })]),
    });
    const first = renderLocal({ onSwitchModel });
    const box = await composerBox();
    await waitFor(() => expect(attachButton()).toBeEnabled());
    const shot = pngFile(800, 600, 'shot.png');
    fireEvent.paste(box, { clipboardData: { files: [shot] } });
    await waitFor(() => expect(within(strip()).getByText(/~475 tokens/)).toBeInTheDocument());
    await user.type(box, 'half a thought');

    await user.selectOptions(screen.getByRole('combobox', { name: 'Model' }), 'llama-3.2-3b');
    const { draft } = onSwitchModel.mock.calls[0][1]();
    expect(draft).toEqual({ text: 'half a thought', images: [shot] });
    first.unmount();

    renderLocal({ draft });
    await waitFor(() => expect(within(strip()).getByRole('img', { name: 'shot.png' })).toBeInTheDocument());
    await waitFor(() => expect(within(strip()).getByText(/~475 tokens/)).toBeInTheDocument());
    await waitFor(() => expect(screen.getByRole('textbox', { name: 'Message' })).toHaveValue('half a thought'));
    expect(uploadAttachment).toHaveBeenCalledTimes(1);
    act(() => ingestServerEvent({ type: 'stopped', modelId: '8', port: 5555, updatedAt: Date.now(), modelName: 'llama-3.2-3b' }));
  });
});
