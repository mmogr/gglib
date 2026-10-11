import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { DrawingSettings } from '../../../src/components/SettingsModal/fields/DrawingSettings';
import type { GgufModel } from '../../../src/types';

const models = [
  { id: 1, name: 'qwen' },
  { id: 2, name: 'flux', imageFamily: 'flux1' },
  { id: 3, name: 'sdxl', imageFamily: 'sdxl' },
] as GgufModel[];

const optionLabels = () =>
  screen
    .getAllByRole('option')
    .map((option) => option.textContent);

describe('DrawingSettings', () => {
  it('lists only the models that draw, after a no-default choice', () => {
    render(
      <DrawingSettings
        values={{ defaultImageModel: '', mcpDrawing: false }}
        onChange={vi.fn()}
        models={models}
        loadingModels={false}
        saving={false}
      />,
    );

    expect(optionLabels()).toEqual(['No default image model', 'flux', 'sdxl']);
  });

  it('keeps an option for a stored id that is no image model here', () => {
    render(
      <DrawingSettings
        values={{ defaultImageModel: '1', mcpDrawing: false }}
        onChange={vi.fn()}
        models={models}
        loadingModels={false}
        saving={false}
      />,
    );

    expect(screen.getByRole('combobox')).toHaveValue('1');
    expect(optionLabels()).toContain('Model 1 (not an image model in this library)');
  });

  it('shows the switch off when off, and reports a click as on', async () => {
    const onChange = vi.fn();
    render(
      <DrawingSettings
        values={{ defaultImageModel: '', mcpDrawing: false }}
        onChange={onChange}
        models={models}
        loadingModels={false}
        saving={false}
      />,
    );
    const toggle = screen.getByRole('checkbox', { name: /allow mcp clients to draw/i });

    expect(toggle).not.toBeChecked();
    await userEvent.setup().click(toggle);

    expect(onChange).toHaveBeenCalledWith('mcpDrawing', true);
  });
});
