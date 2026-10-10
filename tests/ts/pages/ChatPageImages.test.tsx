/**
 * Images on the chat page: attached by paste, drop or the attach button,
 * each shown in the composer with what it costs, or a refusal said in a
 * toast; offered only where the model reads images, in an edit's composer
 * too; a saved turn's images read
 * from the chat's store with the page's credential, enlarged on a click,
 * and let go when they leave; a reply's tool images in its body, live and
 * reopened, read from the store by id; and the images of an unsent message
 * carried over a model switch without being uploaded again.
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
import { agentRun, chatTransport, conversation, wrapper as pageWrapper, type ChatFixture } from './chatPageHarness';
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
  const { getThread } = transport.current as { getThread: (id: number) => Promise<{ messages: ChatMessage[] }> };
  await waitFor(() => expect(getThread).toHaveBeenCalledWith(1));
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

describe('ChatPage, a tool\'s images', () => {
  const DRAWN = 'd'.repeat(64);
  const DRAWN_TOO = 'e'.repeat(64);
  const DRAWN_LATER = 'f'.repeat(64);
  const drawn = [
    { id: DRAWN, mime: 'image/png', width: 1024, height: 1024 },
    { id: DRAWN_TOO, mime: 'image/jpeg', width: 640, height: 480 },
  ];
  const drawnLater = [{ id: DRAWN_LATER, mime: 'image/png', width: 512, height: 512 }];
  const asked: ChatMessage = {
    id: 11, conversation_id: 1, role: 'user', content: 'Draw a fox.', created_at: '2026-10-09T09:00:00Z',
  };
  /** A tool row, as a reply's tool result is saved. */
  const toolRow = (id: number, callId: string, images?: typeof drawn): ChatMessage => ({
    id, conversation_id: 1, role: 'tool', content: '[image stored]', created_at: '2026-10-09T09:00:09Z',
    metadata: { tool_call_id: callId },
    ...(images && { images }),
  });
  /** A saved reply with `text` that called `draw` once per id in `calls`. */
  const savedReply = (text: string, calls: string[]): ChatMessage => ({
    id: 12, conversation_id: 1, role: 'assistant', content: text, created_at: '2026-10-09T09:00:05Z',
    metadata: { tool_calls: calls.map((id) => ({ id, name: 'draw', arguments: { prompt: 'a fox' } })) },
  });
  const strips = () => screen.queryAllByRole('group', { name: 'Images from tools' });
  const strip = () => screen.getByRole('group', { name: 'Images from tools' });
  const shownSizes = () => within(strip()).getAllByRole('img').map((img) => img.getAttribute('alt'));

  it('shows the images of a live tool result in the reply\'s body, read from the store by id, not in the tool row', async () => {
    fixture.rows = { 1: [asked] };
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [
      { type: 'tool_call_start', tool_call: { id: 'draw_1', name: 'draw', arguments: { prompt: 'a fox' } }, display_name: 'Draw' },
      {
        type: 'tool_call_complete',
        tool_name: 'draw',
        result: { tool_call_id: 'draw_1', content: '[image 1024x1024 PNG stored]', success: true, images: drawn },
        wait_ms: 0, execute_duration_ms: 900, display_name: 'Draw', duration_display: '900ms',
      },
    ];
    page({ imageInput: false });
    renderLocal();

    // The thread can remount while the run is read, so the strip is awaited whole.
    await waitFor(() => {
      expect(shownSizes()).toEqual(['Image, 1024 × 1024', 'Image, 640 × 480']);
      expect(within(strip()).getByRole('img', { name: 'Image, 1024 × 1024' })).toHaveAttribute('src', expect.stringMatching(/^blob:shown-/));
    });
    expect(strip()).toBeVisible();
    expect(fetchAttachmentBlob).toHaveBeenCalledWith('this', DRAWN);
    expect(fetchAttachmentBlob).toHaveBeenCalledWith('this', DRAWN_TOO);
    // Only by id from this machine's store: nothing else is asked for.
    expect(new Set(fetchAttachmentBlob.mock.calls.map(([source, id]) => `${source}:${id}`))).toEqual(new Set([`this:${DRAWN}`, `this:${DRAWN_TOO}`]));
    const tools = screen.getByRole('list', { name: 'Tool execution status' });
    expect(within(tools).queryByRole('img')).not.toBeInTheDocument();
    expect(tools).not.toContainElement(strip());
  });

  it('shows a finished reply\'s tool images under its text while "How this was made" is closed, once, in the order made', async () => {
    const user = userEvent.setup();
    fixture.rows = {
      1: [asked, savedReply('Here is your fox.', ['draw_1', 'draw_2']), toolRow(13, 'draw_1', drawn), toolRow(14, 'draw_2', drawnLater)],
    };
    page({ imageInput: false });
    renderLocal();

    const text = await screen.findByText('Here is your fox.');
    await waitFor(() => expect(within(strip()).getAllByRole('img')).toHaveLength(3));
    expect(strips()).toHaveLength(1);
    expect(shownSizes()).toEqual(['Image, 1024 × 1024', 'Image, 640 × 480', 'Image, 512 × 512']);

    const toggle = screen.getByRole('button', { name: 'How this was made' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    const detail = document.getElementById(toggle.getAttribute('aria-controls')!)!;
    expect(detail).not.toBeVisible();
    expect(strip()).toBeVisible();
    expect(detail).not.toContainElement(strip());
    // Under the text, not above it.
    expect(text.compareDocumentPosition(strip()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    // Opening the detail shows the tool's row, and the images are not drawn again.
    await user.click(toggle);
    expect(detail).toBeVisible();
    expect(within(detail).queryByRole('img')).not.toBeInTheDocument();
    expect(strips()).toHaveLength(1);
  });

  it('shows the same images when a reply without text is reopened, enlarged on a click', async () => {
    const user = userEvent.setup();
    fixture.rows = { 1: [asked, savedReply('', ['draw_1']), toolRow(13, 'draw_1', drawn)] };
    page({ imageInput: false });
    renderLocal();

    await waitFor(() => expect(within(strip()).getAllByRole('img')).toHaveLength(2));
    expect(strip()).toBeVisible();
    expect(fetchAttachmentBlob).toHaveBeenCalledWith('this', DRAWN);
    expect(fetchAttachmentBlob).toHaveBeenCalledWith('this', DRAWN_TOO);

    await user.click(within(strip()).getByRole('button', { name: 'Enlarge image, 1024 × 1024' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('img', { name: 'Image, 1024 × 1024' })).toBeInTheDocument();
  });

  it('shows a tool call without images as before: its row, no strip, nothing read', async () => {
    fixture.rows = { 1: [asked, savedReply('', ['draw_1']), toolRow(13, 'draw_1')] };
    page({ imageInput: false });
    renderLocal();

    const rows = await screen.findByRole('list', { name: 'Tool execution status' });
    const [item] = within(rows).getAllByRole('listitem');
    expect(item).toHaveTextContent(/^Draw$/);
    expect(item.children).toHaveLength(1);
    expect(strips()).toHaveLength(0);
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
    expect(fetchAttachmentBlob).not.toHaveBeenCalled();
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
