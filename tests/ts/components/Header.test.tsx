import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import Header from '../../../src/components/Header';

// Mock the ProxyControl component since it has its own complex state
vi.mock('../../../src/components/ProxyControl', () => ({
  default: () => <button data-testid="proxy-control">Proxy Control Mock</button>,
}));

describe('Header', () => {
  const mockOnOpenChat = vi.fn();
  const mockOnOpenSettings = vi.fn();
  const mockOnToggleWorkPanel = vi.fn();

  const defaultProps = {
    onOpenChat: mockOnOpenChat,
    onOpenSettings: mockOnOpenSettings,
    onToggleWorkPanel: mockOnToggleWorkPanel,
    isWorkPanelVisible: false,
    isModelRunning: false,
  };

  beforeEach(() => {
    mockOnOpenChat.mockClear();
    mockOnOpenSettings.mockClear();
    mockOnToggleWorkPanel.mockClear();
  });

  describe('rendering', () => {
    it('renders the app title', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByText('GGLib')).toBeInTheDocument();
    });

    it('renders the logo emoji', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByText('🦀')).toBeInTheDocument();
    });

    it('renders the chat button', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByRole('button', { name: /chat/i })).toBeInTheDocument();
    });

    it('renders the settings button', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByRole('button', { name: /settings/i })).toBeInTheDocument();
    });

    it('renders the work panel button', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByText('Work Panel')).toBeInTheDocument();
    });

    it('renders the proxy control component', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByTestId('proxy-control')).toBeInTheDocument();
    });
  });

  describe('chat button', () => {
    it('calls onOpenChat when clicked', () => {
      render(<Header {...defaultProps} />);
      
      const chatButton = screen.getByRole('button', { name: /chat/i });
      fireEvent.click(chatButton);
      
      expect(mockOnOpenChat).toHaveBeenCalledTimes(1);
    });

    it('has correct title attribute', () => {
      render(<Header {...defaultProps} />);
      
      const chatButton = screen.getByRole('button', { name: /chat/i });
      expect(chatButton).toHaveAttribute('title', 'Open chat');
    });
  });

  describe('settings button', () => {
    it('calls onOpenSettings when clicked', () => {
      render(<Header {...defaultProps} />);
      
      const settingsButton = screen.getByRole('button', { name: /settings/i });
      fireEvent.click(settingsButton);
      
      expect(mockOnOpenSettings).toHaveBeenCalledTimes(1);
    });

    it('has correct title attribute', () => {
      render(<Header {...defaultProps} />);
      
      const settingsButton = screen.getByRole('button', { name: /settings/i });
      expect(settingsButton).toHaveAttribute('title', 'Open settings');
    });

    it('has correct aria-label', () => {
      render(<Header {...defaultProps} />);
      
      const settingsButton = screen.getByRole('button', { name: /settings/i });
      expect(settingsButton).toHaveAttribute('aria-label', 'Open settings');
    });
  });

  describe('work panel button', () => {
    it('calls onToggleWorkPanel when clicked', () => {
      render(<Header {...defaultProps} />);
      
      const workPanelButton = screen.getByText('Work Panel').closest('button');
      fireEvent.click(workPanelButton!);
      
      expect(mockOnToggleWorkPanel).toHaveBeenCalledTimes(1);
    });

    it('shows "Show work panel" title when panel is hidden', () => {
      render(<Header {...defaultProps} isWorkPanelVisible={false} />);
      
      const workPanelButton = screen.getByText('Work Panel').closest('button');
      expect(workPanelButton).toHaveAttribute('title', 'Show work panel');
    });

    it('shows "Hide work panel" title when panel is visible', () => {
      render(<Header {...defaultProps} isWorkPanelVisible={true} />);
      
      const workPanelButton = screen.getByText('Work Panel').closest('button');
      expect(workPanelButton).toHaveAttribute('title', 'Hide work panel');
    });

    it('has aria-pressed false when panel is hidden', () => {
      render(<Header {...defaultProps} isWorkPanelVisible={false} />);
      
      const workPanelButton = screen.getByText('Work Panel').closest('button');
      expect(workPanelButton).toHaveAttribute('aria-pressed', 'false');
    });

    it('has aria-pressed true when panel is visible', () => {
      render(<Header {...defaultProps} isWorkPanelVisible={true} />);
      
      const workPanelButton = screen.getByText('Work Panel').closest('button');
      expect(workPanelButton).toHaveAttribute('aria-pressed', 'true');
    });

    it('includes model status in aria-label when no model running', () => {
      render(<Header {...defaultProps} isModelRunning={false} />);
      
      const workPanelButton = screen.getByText('Work Panel').closest('button');
      expect(workPanelButton?.getAttribute('aria-label')).toContain('No models running');
    });

    it('includes model status in aria-label when model is running', () => {
      render(<Header {...defaultProps} isModelRunning={true} />);
      
      const workPanelButton = screen.getByText('Work Panel').closest('button');
      expect(workPanelButton?.getAttribute('aria-label')).toContain('A model is running');
    });
  });

  describe('accessibility', () => {
    it('renders header element', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByRole('banner')).toBeInTheDocument();
    });

    it('renders heading for app title', () => {
      render(<Header {...defaultProps} />);
      
      expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('GGLib');
    });

    it('all interactive elements are focusable', () => {
      render(<Header {...defaultProps} />);
      
      const buttons = screen.getAllByRole('button');
      expect(buttons.length).toBeGreaterThanOrEqual(3); // Chat, Settings, Work Panel (+ mocked Proxy)
    });
  });
});
