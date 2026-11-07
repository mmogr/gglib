/**
 * The picture sheet's rules that a chat does not reach easily: which image
 * models "Drawn with" offers, which it starts at, and the refusal to draw
 * with no words. The sheet in a chat is `places/chat/ChatPlacePicture.test.tsx`'s.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRef } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';

import type { PictureRedraw } from '../../../src/services/transport/api/images';

const transport = vi.hoisted(() => ({
  redrawPicture: vi.fn(),
  fetchAttachmentBlob: vi.fn(),
}));
vi.mock('../../../src/services/transport', () => ({ getTransport: () => transport }));

import { PictureSheet } from '../../../src/components/ChatMessagesPanel/components/PictureSheet';
import type { ImageModelRow } from '../../../src/types/generated/ImageModelRow';

const PICTURE = { id: 'a'.repeat(64), mime: 'image/png', width: 1000, height: 500 };
const model = (id: number, name: string, complete = true): ImageModelRow => ({
  id,
  name,
  family: 'flux1',
  complete,
  missing: [],
  default: id === 1,
  resident: false,
});

let sent: PictureRedraw | undefined;

beforeEach(() => {
  vi.clearAllMocks();
  sent = undefined;
  transport.fetchAttachmentBlob.mockResolvedValue(new Blob(['picture']));
  transport.redrawPicture.mockImplementation((redraw: PictureRedraw) => {
    sent = redraw;
    return vi.fn();
  });
});

function sheet(models: ImageModelRow[], picked: number | null = null) {
  render(
    <PictureSheet
      url="blob:original"
      size="1000 × 500"
      drawn={{ image: PICTURE, prompt: 'a red fox' }}
      pictures={{ conversationId: 7, models, picked, onKept: vi.fn() }}
      from={createRef()}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByRole('button', { name: 'Redo' }));
}

async function drawn(): Promise<PictureRedraw> {
  fireEvent.click(screen.getByRole('button', { name: 'Draw it again' }));
  await waitFor(() => expect(sent).toBeDefined());
  return sent!;
}

describe('the picture sheet, Drawn with', () => {
  it('offers the default and each image model that can draw, and sends the one chosen', async () => {
    sheet([model(1, 'flux'), model(2, 'qwen-image'), model(3, 'half', false)]);
    const menu = screen.getByLabelText('Image model');
    expect([...menu.querySelectorAll('option')].map((o) => o.value)).toEqual(['default', '1', '2']);

    fireEvent.change(menu, { target: { value: '2' } });

    expect((await drawn()).model).toBe(2);
  });

  it('starts at the image model the chat has chosen', async () => {
    sheet([model(1, 'flux'), model(2, 'qwen-image')], 2);
    expect(screen.getByLabelText('Image model')).toHaveValue('2');
    expect((await drawn()).model).toBe(2);
  });

  it("falls back to the machine's own choice for a chosen model it can no longer draw with", async () => {
    sheet([model(1, 'flux'), model(2, 'qwen-image'), model(3, 'half', false)], 3);
    expect(screen.getByLabelText('Image model')).toHaveValue('default');
    expect(await drawn()).not.toHaveProperty('model');
  });

  it('is not shown where only one image model can draw', () => {
    sheet([model(1, 'flux'), model(3, 'half', false)]);
    expect(screen.queryByLabelText('Image model')).not.toBeInTheDocument();
  });
});

describe('the picture sheet, with no words', () => {
  it('will not draw, and says why', () => {
    sheet([]);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '   ' } });

    expect(screen.getByRole('button', { name: 'Draw it again' })).toBeDisabled();
    expect(screen.getByText('Say what the picture should show.')).toBeInTheDocument();
  });
});
