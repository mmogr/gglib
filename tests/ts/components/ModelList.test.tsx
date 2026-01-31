import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom';
import ModelList from '../../../src/components/ModelList';
import { removeModel } from '../../../src/services/clients/models';
import { serveModel } from '../../../src/services/clients/servers';
import type { GgufModel } from '../../../src/types';
import { MOCK_BASE_PORT } from '../fixtures/ports';

// Mock clients service functions
vi.mock('../../../src/services/clients/models', () => ({
  removeModel: vi.fn(),
}));

vi.mock('../../../src/services/clients/servers', () => ({
  serveModel: vi.fn(),
}));

// Mock window.confirm and window.alert
const mockConfirm = vi.fn();
const mockAlert = vi.fn();
const originalConfirm = window.confirm;
const originalAlert = window.alert;

describe('ModelList', () => {
  const mockOnRefresh = vi.fn();
  const mockOnModelRemoved = vi.fn();

  const createModel = (overrides: Partial<GgufModel> = {}): GgufModel => ({
    id: 1,
    name: 'TestModel',
    file_path: '/path/to/model.gguf',
    param_count_b: 7.0,
    architecture: 'llama',
    quantization: 'Q4_K_M',
    context_length: 4096,
    added_at: '2024-01-15T10:00:00Z',
    hf_repo_id: 'user/repo',
    ...overrides,
  });

  const defaultProps = {
    models: [createModel()],
    loading: false,
    error: null,
    onRefresh: mockOnRefresh,
    onModelRemoved: mockOnModelRemoved,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    window.confirm = mockConfirm;
    window.alert = mockAlert;
  });

  afterEach(() => {
    window.confirm = originalConfirm;
    window.alert = originalAlert;
  });

  describe('rendering', () => {
    it('renders model count in header', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByText('Your Models (1)')).toBeInTheDocument();
    });

    it('renders multiple models with correct count', () => {
      const models = [
        createModel({ id: 1, name: 'Model1' }),
        createModel({ id: 2, name: 'Model2' }),
        createModel({ id: 3, name: 'Model3' }),
      ];
      render(<ModelList {...defaultProps} models={models} />);
      
      expect(screen.getByText('Your Models (3)')).toBeInTheDocument();
    });

    it('renders model name', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByText('TestModel')).toBeInTheDocument();
    });

    it('renders formatted size for models >= 1B', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByText('7.0B')).toBeInTheDocument();
    });

    it('renders formatted size for models < 1B', () => {
      const smallModel = createModel({ param_count_b: 0.5 });
      render(<ModelList {...defaultProps} models={[smallModel]} />);
      
      expect(screen.getByText('500M')).toBeInTheDocument();
    });

    it('renders architecture', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByText('llama')).toBeInTheDocument();
    });

    it('renders quantization', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByText('Q4_K_M')).toBeInTheDocument();
    });

    it('renders HuggingFace repo ID', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByText('📦 user/repo')).toBeInTheDocument();
    });

    it('renders dash for missing architecture', () => {
      const model = createModel({ architecture: undefined });
      render(<ModelList {...defaultProps} models={[model]} />);
      
      expect(screen.getByText('—')).toBeInTheDocument();
    });

    it('renders table headers', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByText('Name')).toBeInTheDocument();
      expect(screen.getByText('Size')).toBeInTheDocument();
      expect(screen.getByText('Architecture')).toBeInTheDocument();
      expect(screen.getByText('Quantization')).toBeInTheDocument();
      expect(screen.getByText('Added')).toBeInTheDocument();
      expect(screen.getByText('Actions')).toBeInTheDocument();
    });

    it('renders serve and remove buttons', () => {
      render(<ModelList {...defaultProps} />);
      
      expect(screen.getByTitle('Serve model')).toBeInTheDocument();
      expect(screen.getByTitle('Remove model')).toBeInTheDocument();
    });
  });

  describe('loading state', () => {
    it('shows loading message when loading with no models', () => {
      render(<ModelList {...defaultProps} models={[]} loading={true} />);
      
      expect(screen.getByText('Loading models...')).toBeInTheDocument();
    });

    it('shows models when loading with existing models', () => {
      render(<ModelList {...defaultProps} loading={true} />);
      
      expect(screen.getByText('TestModel')).toBeInTheDocument();
    });

    it('disables refresh button when loading', () => {
      render(<ModelList {...defaultProps} loading={true} />);
      
      const refreshButton = screen.getByText('Loading...');
      expect(refreshButton).toBeDisabled();
    });

    it('enables refresh button when not loading', () => {
      render(<ModelList {...defaultProps} />);
      
      const refreshButton = screen.getByText('🔄 Refresh');
      expect(refreshButton).not.toBeDisabled();
    });
  });

  describe('empty state', () => {
    it('shows empty state message when no models', () => {
      render(<ModelList {...defaultProps} models={[]} />);
      
      expect(screen.getByText('No models found. Add your first model to get started!')).toBeInTheDocument();
    });
  });

  describe('error state', () => {
    it('shows error message when error is set', () => {
      render(<ModelList {...defaultProps} error="Failed to load models" />);
      
      expect(screen.getByText('Error: Failed to load models')).toBeInTheDocument();
    });

    it('shows retry button on error', () => {
      render(<ModelList {...defaultProps} error="Failed to load models" />);
      
      expect(screen.getByText('Retry')).toBeInTheDocument();
    });

    it('calls onRefresh when retry button clicked', () => {
      render(<ModelList {...defaultProps} error="Failed to load models" />);
      
      const retryButton = screen.getByText('Retry');
      fireEvent.click(retryButton);
      
      expect(mockOnRefresh).toHaveBeenCalledTimes(1);
    });

    it('does not show models when error is set', () => {
      render(<ModelList {...defaultProps} error="Failed to load models" />);
      
      expect(screen.queryByText('TestModel')).not.toBeInTheDocument();
    });
  });

  describe('refresh functionality', () => {
    it('calls onRefresh when refresh button clicked', () => {
      render(<ModelList {...defaultProps} />);
      
      const refreshButton = screen.getByText('🔄 Refresh');
      fireEvent.click(refreshButton);
      
      expect(mockOnRefresh).toHaveBeenCalledTimes(1);
    });
  });

  describe('remove model', () => {
    it('shows confirmation dialog before removing', () => {
      mockConfirm.mockReturnValue(false);
      render(<ModelList {...defaultProps} />);
      
      const removeButton = screen.getByTitle('Remove model');
      fireEvent.click(removeButton);
      
      expect(mockConfirm).toHaveBeenCalledWith('Are you sure you want to remove "TestModel"?');
    });

    it('does not remove model when confirmation cancelled', () => {
      mockConfirm.mockReturnValue(false);
      render(<ModelList {...defaultProps} />);
      
      const removeButton = screen.getByTitle('Remove model');
      fireEvent.click(removeButton);
      
      expect(removeModel).not.toHaveBeenCalled();
    });

    it('calls removeModel when confirmed', async () => {
      mockConfirm.mockReturnValue(true);
      vi.mocked(removeModel).mockResolvedValue(undefined);
      
      render(<ModelList {...defaultProps} />);
      
      const removeButton = screen.getByTitle('Remove model');
      fireEvent.click(removeButton);
      
      await waitFor(() => {
        expect(removeModel).toHaveBeenCalledWith(1);
      });
    });

    it('calls onModelRemoved after successful removal', async () => {
      mockConfirm.mockReturnValue(true);
      vi.mocked(removeModel).mockResolvedValue(undefined);
      
      render(<ModelList {...defaultProps} />);
      
      const removeButton = screen.getByTitle('Remove model');
      fireEvent.click(removeButton);
      
      await waitFor(() => {
        expect(mockOnModelRemoved).toHaveBeenCalledTimes(1);
      });
    });

    it('shows alert on removal error', async () => {
      mockConfirm.mockReturnValue(true);
      vi.mocked(removeModel).mockRejectedValue(new Error('Removal failed'));
      
      render(<ModelList {...defaultProps} />);
      
      const removeButton = screen.getByTitle('Remove model');
      fireEvent.click(removeButton);
      
      await waitFor(() => {
        expect(mockAlert).toHaveBeenCalledWith('Failed to remove model: Error: Removal failed');
      });
    });

    it('does not call onModelRemoved on removal error', async () => {
      mockConfirm.mockReturnValue(true);
      vi.mocked(removeModel).mockRejectedValue(new Error('Removal failed'));
      
      render(<ModelList {...defaultProps} />);
      
      const removeButton = screen.getByTitle('Remove model');
      fireEvent.click(removeButton);
      
      await waitFor(() => {
        expect(mockAlert).toHaveBeenCalled();
      });
      
      expect(mockOnModelRemoved).not.toHaveBeenCalled();
    });

    it('does not try to remove model without id', () => {
      const modelNoId = createModel({ id: undefined });
      render(<ModelList {...defaultProps} models={[modelNoId]} />);
      
      const removeButton = screen.getByTitle('Remove model');
      fireEvent.click(removeButton);
      
      expect(mockConfirm).not.toHaveBeenCalled();
      expect(removeModel).not.toHaveBeenCalled();
    });
  });

  describe('serve model modal', () => {
    it('opens serve modal when serve button clicked', () => {
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      expect(screen.getByText('Start Model Server')).toBeInTheDocument();
    });

    it('shows model name in serve modal', () => {
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      // Modal has model name in a strong element
      const modelNames = screen.getAllByText('TestModel');
      expect(modelNames.length).toBeGreaterThanOrEqual(1);
      // Check that at least one is inside a strong element (in the modal)
      const modalModelName = modelNames.find(el => el.tagName === 'STRONG');
      expect(modalModelName).toBeInTheDocument();
    });

    it('shows model size in serve modal', () => {
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      // The modal shows size in a separate element
      const sizeElements = screen.getAllByText('7.0B');
      expect(sizeElements.length).toBeGreaterThanOrEqual(1);
    });

    it('shows context length input', () => {
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      expect(screen.getByLabelText(/Context Length/i)).toBeInTheDocument();
    });

    it('shows default context length placeholder', () => {
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      const contextInput = screen.getByLabelText(/Context Length/i);
      expect(contextInput).toHaveAttribute('placeholder', 'Default: 4,096');
    });

    it('closes modal when cancel button clicked', () => {
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      expect(screen.getByText('Start Model Server')).toBeInTheDocument();
      
      const cancelButton = screen.getByText('Cancel');
      fireEvent.click(cancelButton);
      
      expect(screen.queryByText('Start Model Server')).not.toBeInTheDocument();
    });

    it('closes modal when X button clicked', () => {
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      const closeButton = screen.getByText('✕');
      fireEvent.click(closeButton);
      
      expect(screen.queryByText('Start Model Server')).not.toBeInTheDocument();
    });

    it('calls serveModel with correct params when start button clicked', async () => {
      vi.mocked(serveModel).mockResolvedValue({ port: MOCK_BASE_PORT, message: 'Server started' });
      
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      const startButton = screen.getByText('Start Server');
      fireEvent.click(startButton);
      
      await waitFor(() => {
        expect(serveModel).toHaveBeenCalledWith({
          id: 1,
          context_length: 4096,
          mlock: false,
          jinja: false, // No agent/reasoning tags, so jinja defaults to false
        });
      });
    });

    it('uses custom context length when provided', async () => {
      vi.mocked(serveModel).mockResolvedValue({ port: MOCK_BASE_PORT, message: 'Server started' });
      
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      const contextInput = screen.getByLabelText(/Context Length/i);
      fireEvent.change(contextInput, { target: { value: '8192' } });
      
      const startButton = screen.getByText('Start Server');
      fireEvent.click(startButton);
      
      await waitFor(() => {
        expect(serveModel).toHaveBeenCalledWith({
          id: 1,
          context_length: 8192,
          mlock: false,
          jinja: false, // No agent/reasoning tags
        });
      });
    });

    it('closes modal and calls onRefresh after successful serve', async () => {
      vi.mocked(serveModel).mockResolvedValue({ port: MOCK_BASE_PORT, message: 'Server started' });
      
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      const startButton = screen.getByText('Start Server');
      fireEvent.click(startButton);
      
      await waitFor(() => {
        expect(screen.queryByText('Start Model Server')).not.toBeInTheDocument();
      });
      
      expect(mockOnRefresh).toHaveBeenCalled();
    });

    it('shows alert on serve error', async () => {
      vi.mocked(serveModel).mockRejectedValue(new Error('Serve failed'));
      
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      const startButton = screen.getByText('Start Server');
      fireEvent.click(startButton);
      
      await waitFor(() => {
        expect(mockAlert).toHaveBeenCalledWith('Failed to serve model: Error: Serve failed');
      });
    });

    it('shows loading state during serve', async () => {
      let resolveServe: () => void;
      const servePromise = new Promise<{ port: number; message: string }>((resolve) => {
        resolveServe = () => resolve({ port: MOCK_BASE_PORT, message: 'Server started' });
      });
      vi.mocked(serveModel).mockReturnValue(servePromise);
      
      render(<ModelList {...defaultProps} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      const startButton = screen.getByText('Start Server');
      fireEvent.click(startButton);
      
      await waitFor(() => {
        expect(screen.getByText('Loading model...')).toBeInTheDocument();
      });
      
      resolveServe!();
    });

    it('does not open serve modal for model without id', () => {
      const modelNoId = createModel({ id: undefined });
      render(<ModelList {...defaultProps} models={[modelNoId]} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      expect(screen.queryByText('Start Model Server')).not.toBeInTheDocument();
    });

    it('auto-enables jinja for agent-tagged model', async () => {
      vi.mocked(serveModel).mockResolvedValue({ port: MOCK_BASE_PORT, message: 'Server started' });
      
      const agentModel = createModel({ tags: ['agent'] });
      render(<ModelList {...defaultProps} models={[agentModel]} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      // Verify agent badge is shown
      expect(screen.getByText('🔧 Agent')).toBeInTheDocument();
      
      const startButton = screen.getByText('Start Server');
      fireEvent.click(startButton);
      
      await waitFor(() => {
        expect(serveModel).toHaveBeenCalledWith({
          id: 1,
          context_length: 4096,
          mlock: false,
          jinja: true, // Auto-enabled for agent tag
        });
      });
    });

    it('auto-enables jinja for reasoning-tagged model', async () => {
      vi.mocked(serveModel).mockResolvedValue({ port: MOCK_BASE_PORT, message: 'Server started' });
      
      const reasoningModel = createModel({ tags: ['reasoning'] });
      render(<ModelList {...defaultProps} models={[reasoningModel]} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      // Verify reasoning badge is shown
      expect(screen.getByText('🧠 Reasoning')).toBeInTheDocument();
      
      const startButton = screen.getByText('Start Server');
      fireEvent.click(startButton);
      
      await waitFor(() => {
        expect(serveModel).toHaveBeenCalledWith({
          id: 1,
          context_length: 4096,
          mlock: false,
          jinja: true, // Auto-enabled for reasoning tag
        });
      });
    });

    it('shows both badges for model with both tags', () => {
      const dualCapModel = createModel({ tags: ['agent', 'reasoning'] });
      render(<ModelList {...defaultProps} models={[dualCapModel]} />);
      
      const serveButton = screen.getByTitle('Serve model');
      fireEvent.click(serveButton);
      
      expect(screen.getByText('🧠 Reasoning')).toBeInTheDocument();
      expect(screen.getByText('🔧 Agent')).toBeInTheDocument();
    });
  });
});
