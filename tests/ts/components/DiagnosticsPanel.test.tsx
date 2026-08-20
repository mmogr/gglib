/**
 * Tests for the diagnostics panel — the GUI face of `gglib config
 * check-deps` / `paths` / `fast-downloads status`.
 *
 * The load-bearing behaviours: missing *required* dependencies are called out
 * with their install command, an optional one is not treated as a failure,
 * and the accelerator reads as a speed setting rather than a requirement.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DiagnosticsPanel } from '../../../src/components/SettingsModal/DiagnosticsPanel';
import {
  getDiagnostics,
  disableFastDownloads,
  enableFastDownloads,
} from '../../../src/services/transport/api/setup';
import type { Diagnostics } from '../../../src/types/setup';

vi.mock('../../../src/services/transport/api/setup', () => ({
  getDiagnostics: vi.fn(),
  enableFastDownloads: vi.fn().mockResolvedValue(undefined),
  disableFastDownloads: vi.fn().mockResolvedValue({ removed: true }),
}));

const base: Diagnostics = {
  dependencies: [
    {
      name: 'cmake',
      status: 'present',
      version: '3.28.1',
      description: 'Build',
      required: true,
      installHint: null,
    },
    {
      name: 'git',
      status: 'missing',
      version: null,
      description: 'Version control',
      required: true,
      installHint: 'brew install git',
    },
    {
      name: 'python3',
      status: 'missing',
      version: null,
      description: 'Optional helper',
      required: false,
      installHint: null,
    },
  ],
  paths: {
    dataRoot: '/home/u/.local/share/gglib',
    resourceRoot: '/home/u/.local/share/gglib',
    databasePath: '/home/u/.local/share/gglib/gglib.db',
    llamaServerPath: '/home/u/.local/share/gglib/bin/llama-server',
    modelsDir: '/models',
    modelsSource: 'envVar',
  },
  acceleration: { detected: 'Metal', detectionError: null },
  fastDownloads: {
    provisioned: false,
    envDir: '/home/u/.local/share/gglib/py',
    legacyPath: false,
    builder: null,
    availableBuilder: 'uv',
    error: null,
  },
};

const mockGet = vi.mocked(getDiagnostics);

describe('DiagnosticsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGet.mockResolvedValue(base);
  });

  it('calls out missing required dependencies with their install command', async () => {
    render(<DiagnosticsPanel />);

    expect(await screen.findByText(/1 required dependency is missing/)).toBeInTheDocument();
    expect(screen.getByText('brew install git')).toBeInTheDocument();
  });

  it('prints a shared install command once, not once per dependency', async () => {
    mockGet.mockResolvedValue({
      ...base,
      dependencies: [
        {
          name: 'git',
          status: 'missing',
          version: null,
          description: 'a',
          required: true,
          installHint: 'apt install build-essential',
        },
        {
          name: 'cmake',
          status: 'missing',
          version: null,
          description: 'b',
          required: true,
          installHint: 'apt install build-essential',
        },
      ],
    });

    render(<DiagnosticsPanel />);

    expect(await screen.findAllByText('apt install build-essential')).toHaveLength(1);
  });

  it('attributes missing dependencies to building gglib, not to llama.cpp', async () => {
    render(<DiagnosticsPanel />);
    expect(await screen.findByText(/build gglib itself from source/)).toBeInTheDocument();
  });

  it('does not count an optional dependency as missing', async () => {
    render(<DiagnosticsPanel />);
    // python3 is also absent, but only `git` is required.
    expect(await screen.findByText(/1 required dependency is missing/)).toBeInTheDocument();
  });

  it('reports a clean run without a warning banner', async () => {
    mockGet.mockResolvedValue({
      ...base,
      dependencies: [base.dependencies[0], base.dependencies[2]],
    });

    render(<DiagnosticsPanel />);

    await screen.findByText('cmake');
    expect(screen.queryByText(/required.*missing/)).not.toBeInTheDocument();
  });

  it('names where the models directory came from', async () => {
    render(<DiagnosticsPanel />);
    expect(
      await screen.findByText(/Models directory from the environment/),
    ).toBeInTheDocument();
  });

  it('surfaces a detection failure instead of implying CPU will be used', async () => {
    mockGet.mockResolvedValue({
      ...base,
      acceleration: { detected: null, detectionError: 'No supported GPU acceleration found' },
    });

    render(<DiagnosticsPanel />);
    expect(await screen.findByText(/No supported GPU acceleration found/)).toBeInTheDocument();
  });

  it('enables the accelerator and re-reads the state', async () => {
    render(<DiagnosticsPanel />);

    await userEvent.click(await screen.findByRole('button', { name: /Enable accelerator/ }));

    await waitFor(() => expect(enableFastDownloads).toHaveBeenCalled());
    // Enabling changes what the panel reports, so it must refetch.
    expect(mockGet).toHaveBeenCalledTimes(2);
  });

  it('offers disabling when already provisioned', async () => {
    mockGet.mockResolvedValue({
      ...base,
      fastDownloads: { ...base.fastDownloads, provisioned: true, builder: 'uv' },
    });

    render(<DiagnosticsPanel />);

    await userEvent.click(await screen.findByRole('button', { name: /Disable accelerator/ }));
    await waitFor(() => expect(disableFastDownloads).toHaveBeenCalled());
  });

  it('keeps a provisioning failure visible with its remedy', async () => {
    vi.mocked(enableFastDownloads).mockRejectedValueOnce(new Error('No Python interpreter found'));

    render(<DiagnosticsPanel />);
    await userEvent.click(await screen.findByRole('button', { name: /Enable accelerator/ }));

    expect(await screen.findByText(/No Python interpreter found/)).toBeInTheDocument();
  });
});
