import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { InferenceProfiles } from '../../../src/components/SettingsModal/InferenceProfiles';
import { profileNameError } from '../../../src/components/SettingsModal/InferenceProfileEditor';
import type { AppSettings, InferenceProfile } from '../../../src/types';

const getSettings = vi.fn();
const updateSettings = vi.fn();

vi.mock('../../../src/services/transport/api/settings', () => ({
  getSettings: (...args: unknown[]) => getSettings(...args),
  updateSettings: (...args: unknown[]) => updateSettings(...args),
}));

function coding(overrides: Partial<InferenceProfile> = {}): InferenceProfile {
  return {
    name: 'coding',
    description: 'Low-variance sampling',
    config: { temperature: 0.2, topP: 0.9 },
    listInModels: false,
    ...overrides,
  };
}

function settings(profiles: InferenceProfile[]): AppSettings {
  return { inferenceProfiles: profiles };
}

beforeEach(() => {
  getSettings.mockReset();
  updateSettings.mockReset();
  getSettings.mockResolvedValue(settings([coding()]));
  updateSettings.mockImplementation((req: { inferenceProfiles: InferenceProfile[] }) =>
    Promise.resolve(settings(req.inferenceProfiles)),
  );
});

describe('InferenceProfiles', () => {
  it('lists configured profiles with the parameters they set', async () => {
    render(<InferenceProfiles />);

    expect(await screen.findByText('coding')).toBeInTheDocument();
    expect(screen.getByText(/temperature=0\.2/)).toBeInTheDocument();
    expect(screen.getByText(/top-p=0\.9/)).toBeInTheDocument();
  });

  it('marks only profiles that are advertised in the model picker', async () => {
    getSettings.mockResolvedValue(settings([coding(), coding({ name: 'chat', listInModels: true })]));
    render(<InferenceProfiles />);

    await screen.findByText('coding');
    // One badge, for the one listed profile.
    expect(screen.getAllByText(/in model picker/i)).toHaveLength(1);
  });

  it('prompts to create one when none are configured', async () => {
    getSettings.mockResolvedValue(settings([]));
    render(<InferenceProfiles />);

    expect(await screen.findByText(/no inference profiles/i)).toBeInTheDocument();
  });

  /**
   * The core contract with the backend: a blank parameter field must be
   * omitted entirely rather than sent as 0, so it falls through to the
   * model's own default instead of overriding it.
   */
  it('omits blank parameters instead of sending zero', async () => {
    getSettings.mockResolvedValue(settings([]));
    const user = userEvent.setup();
    render(<InferenceProfiles />);

    await user.click(await screen.findByRole('button', { name: /add profile/i }));
    await user.type(screen.getByLabelText(/^name$/i), 'chat');
    await user.type(screen.getByLabelText(/temperature/i), '0.7');
    await user.click(screen.getByRole('button', { name: /create profile/i }));

    await waitFor(() => expect(updateSettings).toHaveBeenCalled());
    const sent = updateSettings.mock.calls[0][0].inferenceProfiles[0];
    expect(sent.name).toBe('chat');
    expect(sent.config.temperature).toBe(0.7);
    expect(sent.config).not.toHaveProperty('topP');
    expect(sent.config).not.toHaveProperty('minP');
  });

  it('saves the whole list when deleting, keeping the others', async () => {
    getSettings.mockResolvedValue(settings([coding(), coding({ name: 'chat' })]));
    const user = userEvent.setup();
    render(<InferenceProfiles />);

    await screen.findByText('coding');
    await user.click(screen.getAllByRole('button', { name: /delete/i })[0]);

    await waitFor(() => expect(updateSettings).toHaveBeenCalled());
    const sent = updateSettings.mock.calls[0][0].inferenceProfiles;
    expect(sent.map((p: InferenceProfile) => p.name)).toEqual(['chat']);
  });

  /**
   * The server validates independently and is the authority, so a rejection
   * must surface rather than leaving the UI showing something unsaved.
   */
  it('surfaces a server rejection', async () => {
    getSettings.mockResolvedValue(settings([coding()]));
    updateSettings.mockRejectedValue(new Error('Invalid inference profile: name is reserved'));
    const user = userEvent.setup();
    render(<InferenceProfiles />);

    await screen.findByText('coding');
    await user.click(screen.getByRole('button', { name: /delete/i }));

    expect(await screen.findByText(/invalid inference profile/i)).toBeInTheDocument();
  });
});

describe('profileNameError', () => {
  it('accepts a valid slug', () => {
    expect(profileNameError('long-form', [])).toBeNull();
  });

  it.each([
    ['', /required/i],
    ['Coding', /lowercase/i],
    ['long_form', /lowercase/i],
    ['-coding', /start or end/i],
    ['coding-', /start or end/i],
    ['interactive', /reserved/i],
    ['a'.repeat(33), /32 characters/i],
  ])('rejects %s', (name, expected) => {
    expect(profileNameError(name, [])).toMatch(expected);
  });

  it('rejects a name already in use', () => {
    expect(profileNameError('coding', ['coding'])).toMatch(/already exists/i);
  });
});
