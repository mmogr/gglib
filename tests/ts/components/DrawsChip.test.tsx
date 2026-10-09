/**
 * The Draws chip: "Draws · <family>" on a library row and in the inspector's
 * header for a model that draws images, with the roles still missing a file
 * in its title, and on no model that chats.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import '@testing-library/jest-dom';

import ModelsListContent from '../../../src/components/ModelLibraryPanel/ModelsListContent';
import { InspectorHeader } from '../../../src/components/ModelInspectorPanel/components/InspectorHeader';
import { DrawsChip } from '../../../src/components/DrawsChip';
import type { GgufModel } from '../../../src/types';
import { guiModel } from '../fixtures/model';

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
  it('marks only the library rows of models that draw images, by family', () => {
    render(
      <ModelsListContent
        models={[
          guiModel({ id: 1, name: 'flux1-schnell', imageFamily: 'flux1' }),
          guiModel({ id: 2, name: 'qwen-image', imageFamily: 'qwen-image-2.1' }),
          guiModel({ id: 3, name: 'chatty' }),
        ]}
        selectedModelId={null}
        onSelectModel={vi.fn()}
        loading={false}
        servers={[]}
      />,
    );

    expect(within(screen.getByRole('option', { name: /flux1-schnell/ })).getByText('Draws · Flux.1')).toBeInTheDocument();
    expect(
      within(screen.getByRole('option', { name: /qwen-image/ })).getByText('Draws · Qwen-Image 2.1'),
    ).toBeInTheDocument();
    expect(within(screen.getByRole('option', { name: /chatty/ })).queryByText(/Draws/)).not.toBeInTheDocument();
  });

  it("is in the inspector's header for a model that draws, and not for one that chats", () => {
    header({ name: 'sdxl', imageInput: false, imageFamily: 'sdxl' });
    expect(screen.getByText('Draws · SDXL')).toBeInTheDocument();
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
