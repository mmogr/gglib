/**
 * The Vision chip: in the inspector's header for a model that reads images,
 * and on no other. A library row says "sees images" in its line of facts
 * instead (`tests/ts/places/library.test.tsx`).
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';

import { InspectorHeader } from '../../../src/components/ModelInspectorPanel/components/InspectorHeader';

function header(imageInput: boolean, isEditMode = false) {
  render(
    <InspectorHeader
      model={{ name: 'X', imageInput }}
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
