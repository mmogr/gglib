/**
 * The Draws chip: "Draws · <family>" in the inspector's header for a model
 * that draws images, with the roles still missing a file in its title, and
 * on no model that chats. A library row says "draws (family)" in its line of
 * facts instead (`tests/ts/places/library.test.tsx`).
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { InspectorHeader } from '../../../src/components/ModelInspectorPanel/components/InspectorHeader';
import { DrawsChip } from '../../../src/components/DrawsChip';
import type { GgufModel } from '../../../src/types';

function header(model: Parameters<typeof InspectorHeader>[0]['model']) {
  render(
    <InspectorHeader
      model={model}
      hasHfRepo={false}
      isEditMode={false}
      editedName={model.name}
      onEditedNameChange={vi.fn()}
      onVerify={vi.fn()}
      onCheckUpdates={vi.fn()}
    />,
  );
}

describe('the Draws chip', () => {
  it("is in the inspector's header for a model that draws, and not for one that chats", () => {
    header({ name: 'sdxl', imageInput: false, imageFamily: 'sdxl' });
    expect(screen.getByText('Draws · SDXL')).toBeInTheDocument();
  });

  it('names Z-Image by its label and not its wire name', () => {
    header({ name: 'z-image-turbo', imageInput: false, imageFamily: 'z-image' });
    expect(screen.getByText('Draws · Z-Image')).toBeInTheDocument();
    expect(screen.queryByText(/z-image$/)).not.toBeInTheDocument();
  });

  it('names FLUX.3 Image Klein by its label and not its wire name', () => {
    header({ name: 'flux3-image-klein-Q8_0', imageInput: false, imageFamily: 'flux3-klein' });
    expect(screen.getByText('Draws · FLUX 3 Klein')).toBeInTheDocument();
    expect(screen.queryByText(/flux3-klein$/)).not.toBeInTheDocument();
  });

  it('names FLUX.2 klein by its label and not its wire name', () => {
    header({ name: 'flux-2-klein-4b-Q8_0', imageInput: false, imageFamily: 'flux2-klein' });
    expect(screen.getByText('Draws · FLUX.2 klein')).toBeInTheDocument();
    expect(screen.queryByText(/flux2-klein$/)).not.toBeInTheDocument();
  });

  it("is not in the inspector's header for a model that chats", () => {
    header({ name: 'X', imageInput: false });
    expect(screen.getByRole('heading', { name: 'X' })).toBeInTheDocument();
    expect(screen.queryByText(/Draws/)).not.toBeInTheDocument();
  });

  it('names the roles still missing a file in its title, and none when every one is linked', () => {
    const model = (missingComponents: GgufModel['missingComponents']) => ({ imageFamily: 'flux1' as const, missingComponents });
    const { rerender } = render(<DrawsChip model={model(['vae', 't5xxl'])} />);
    expect(screen.getByText('Draws · Flux.1').closest('[title]')).toHaveAttribute(
      'title',
      'Draws images (Flux.1); needs VAE, T5-XXL',
    );

    rerender(<DrawsChip model={model([])} />);
    expect(screen.getByText('Draws · Flux.1').closest('[title]')).toHaveAttribute('title', 'Draws images (Flux.1)');
  });
});
