/**
 * The Vision chip: on a library row and in the inspector's header for a
 * model that reads images, and on no other.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import '@testing-library/jest-dom';

import ModelsListContent from '../../../src/components/ModelLibraryPanel/ModelsListContent';
import { InspectorHeader } from '../../../src/components/ModelInspectorPanel/components/InspectorHeader';
import { guiModel } from '../fixtures/model';

function header(imageInput: boolean, isEditMode = false) {
  render(
    <InspectorHeader
      modelName="X"
      imageInput={imageInput}
      hasHfRepo={false}
      isEditMode={isEditMode}
      editedName="X"
      onEditedNameChange={vi.fn()}
      onVerify={vi.fn()}
      onCheckUpdates={vi.fn()}
    />,
  );
}

describe('the Vision chip', () => {
  it('marks only the library rows of models that read images', () => {
    render(
      <ModelsListContent
        models={[guiModel({ id: 1, name: 'sees', imageInput: true }), guiModel({ id: 2, name: 'text-only' })]}
        selectedModelId={null}
        onSelectModel={vi.fn()}
        loading={false}
        servers={[]}
      />,
    );

    expect(within(screen.getByRole('option', { name: /sees/ })).getByText('Vision')).toBeInTheDocument();
    expect(within(screen.getByRole('option', { name: /text-only/ })).queryByText('Vision')).not.toBeInTheDocument();
  });

  it("is in the inspector's header for a model that reads images", () => {
    header(true);
    expect(screen.getByText('Vision')).toBeInTheDocument();
  });

  it("is not in the inspector's header for a model that does not", () => {
    header(false);
    expect(screen.getByRole('heading', { name: 'X' })).toBeInTheDocument();
    expect(screen.queryByText('Vision')).not.toBeInTheDocument();
  });
});
