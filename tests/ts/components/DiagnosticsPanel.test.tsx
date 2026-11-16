/**
 * Tests for the diagnostics panel — the GUI face of `gglib config
 * check-deps` / `paths` / `fast-downloads status`.
 *
 * The load-bearing behaviours: what this gglib needs and lacks is called out
 * with its install command, in the report's own words; a dependency it does
 * not need is not treated as a failure; and the accelerator reads as a speed
 * setting rather than a requirement.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DiagnosticsPanel } from '../../../src/places/diagnostics/DiagnosticsPanel';
import {
  getDiagnostics,
  disableFastDownloads,
  enableFastDownloads,
} from '../../../src/services/transport/api/setup';
import type {
  DependencyInfo,
  DependencyReport,
  Diagnostics,
} from '../../../src/types/setup';

vi.mock('../../../src/services/transport/api/setup', () => ({
  getDiagnostics: vi.fn(),
  enableFastDownloads: vi.fn().mockResolvedValue(undefined),
  disableFastDownloads: vi.fn().mockResolvedValue({ removed: true }),
}));

/** A report as Rust words it; the panel adds no words of its own. */
const report = (
  dependencies: DependencyInfo[],
  verdict = '1 dependency this gglib needs is missing.',
): DependencyReport => ({
  groups: [
    {
      title: 'Building an engine from source',
      explanation: 'These build llama.cpp or stable-diffusion.cpp from source.',
      needed: true,
      dependencies,
    },
  ],
  verdict,
  complete: !dependencies.some((d) => d.required && d.status === 'missing'),
});

const cmake: DependencyInfo = {
  name: 'cmake',
  status: 'present',
  version: '3.28.1',
  statusWord: 'v3.28.1',
  description: 'Build',
  required: true,
  installHint: null,
};
const git: DependencyInfo = {
  name: 'git',
  status: 'missing',
  version: null,
  statusWord: 'missing',
  description: 'Version control',
  required: true,
  installHint: 'brew install git',
};
const python3: DependencyInfo = {
  name: 'python3',
  status: 'missing',
  version: null,
  statusWord: 'not installed',
  description: 'Optional helper',
  required: false,
  installHint: null,
};

const base: Diagnostics = {
  dependencies: report([cmake, git, python3]),
  paths: {
    dataRoot: '/home/u/.local/share/gglib',
    resourceRoot: '/home/u/.local/share/gglib',
    databasePath: '/home/u/.local/share/gglib/gglib.db',
    llamaServerPath: '/home/u/.local/share/gglib/bin/llama-server',
    sdServerPath: '/home/u/.local/share/gglib/.sd/bin/sd-server',
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
    state: 'off',
  },
};

const mockGet = vi.mocked(getDiagnostics);

describe('DiagnosticsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGet.mockResolvedValue(base);
  });

  it("calls out what this gglib needs with the install command, in the report's words", async () => {
    render(<DiagnosticsPanel />);

    expect(
      await screen.findByText('1 dependency this gglib needs is missing.'),
    ).toBeInTheDocument();
    expect(screen.getByText('brew install git')).toBeInTheDocument();
  });

  it('prints a shared install command once, not once per dependency', async () => {
    const shared = { ...git, installHint: 'apt install build-essential' };
    mockGet.mockResolvedValue({
      ...base,
      dependencies: report(
        [shared, { ...shared, name: 'cmake' }],
        '2 dependencies this gglib needs are missing.',
      ),
    });

    render(<DiagnosticsPanel />);

    expect(await screen.findAllByText('apt install build-essential')).toHaveLength(1);
  });

  it('shows each group under its heading with what it says about this gglib', async () => {
    render(<DiagnosticsPanel />);
    expect(
      await screen.findByRole('heading', { name: 'Building an engine from source' }),
    ).toBeInTheDocument();
    expect(
      screen.getByText('These build llama.cpp or stable-diffusion.cpp from source.'),
    ).toBeInTheDocument();
  });

  it("shows a row's state in the word the report gives it", async () => {
    render(<DiagnosticsPanel />);
    // python3 is absent but not needed: "not installed", not "missing".
    expect(await screen.findByText('not installed')).toBeInTheDocument();
    expect(screen.getByText('v3.28.1')).toBeInTheDocument();
  });

  it('reports a clean run without a warning banner', async () => {
    // An installed copy: its build tools are absent and not needed.
    mockGet.mockResolvedValue({
      ...base,
      dependencies: report(
        [cmake, { ...git, required: false, statusWord: 'not installed' }],
        'Nothing this gglib needs is missing.',
      ),
    });

    render(<DiagnosticsPanel />);

    await screen.findByText('cmake');
    expect(screen.queryByText('Nothing this gglib needs is missing.')).not.toBeInTheDocument();
    expect(screen.queryByText('brew install git')).not.toBeInTheDocument();
  });

  it('names where the models directory came from', async () => {
    render(<DiagnosticsPanel />);
    expect(
      await screen.findByText(/Models directory from the environment/),
    ).toBeInTheDocument();
  });

  it('shows where each managed server binary is', async () => {
    render(<DiagnosticsPanel />);
    expect(await screen.findByText('sd-server')).toBeInTheDocument();
    expect(screen.getByText('/home/u/.local/share/gglib/.sd/bin/sd-server')).toBeInTheDocument();
    expect(screen.getByText('/home/u/.local/share/gglib/bin/llama-server')).toBeInTheDocument();
  });

  it('surfaces a detection failure instead of implying CPU will be used', async () => {
    mockGet.mockResolvedValue({
      ...base,
      acceleration: {
        detected: null,
        detectionError: 'A Vulkan GPU was detected, but a Vulkan build also needs: SPIR-V headers (spirv-headers).',
      },
    });

    render(<DiagnosticsPanel />);
    expect(await screen.findByText(/a Vulkan build also needs: SPIR-V headers/)).toBeInTheDocument();
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
      fastDownloads: { ...base.fastDownloads, provisioned: true, builder: 'uv', state: 'on' },
    });

    render(<DiagnosticsPanel />);
    // The accelerator's state in the served word, the one Settings says.
    expect(await screen.findByText('On')).toBeInTheDocument();
    expect(screen.queryByText('Enabled')).not.toBeInTheDocument();

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
