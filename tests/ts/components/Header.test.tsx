import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import Header from '../../../src/components/Header';
import { ServerInfo } from '../../../src/types';
import { MOCK_PROXY_PORT } from '../fixtures/ports';

// Mock the RunsPopover component since it has its own complex state
vi.mock('../../../src/components/RunsPopover', () => ({
  RunsPopover: ({ isOpen }: { isOpen: boolean }) => 
    isOpen ? <div data-testid="runs-popover">Runs Popover Mock</div> : null,
}));

describe('Header', () => {
  const mockOnOpenSettings = vi.fn();
  const mockOnStopServer = vi.fn().mockResolvedValue(undefined);
  const mockOnSelectModel = vi.fn();
  const mockOnRefreshServers = vi.fn();

  const mockServers: ServerInfo[] = [
    { model_id: 1, model_name: 'Test Model 1', port: MOCK_PROXY_PORT, status: 'running' },
    { model_id: 2, model_name: 'Test Model 2', port: MOCK_PROXY_PORT + 1, status: 'running' },
  ];

  const defaultProps = {
    onOpenSettings: mockOnOpenSettings,
    servers: [] as ServerInfo[],
    onStopServer: mockOnStopServer,
    onSelectModel: mockOnSelectModel,
    onRefreshServers: mockOnRefreshServers,
  };

  beforeEach(() => {
    mockOnOpenSettings.mockClear();
    mockOnStopServer.mockClear();
    mockOnSelectModel.mockClear();
    mockOnRefreshServers.mockClear();
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

    it('renders the settings button in desktop nav', () => {
      render(<Header {...defaultProps} />);
      
      // Both desktop and mobile nav have settings buttons
      const settingsButtons = screen.getAllByRole('button', { name: /settings/i });
      expect(settingsButtons.length).toBeGreaterThanOrEqual(1);
      expect(settingsButtons[0]).toBeInTheDocument();
    });

    it('renders the server status button', () => {
      render(<Header {...defaultProps} />);
      
      // Server status button should be present (disabled when no servers)
      const serverButton = screen.getByLabelText('No servers running');
      expect(serverButton).toBeInTheDocument();
    });
  });

  describe('server status button', () => {
    it('is disabled when no servers are running', () => {
      render(<Header {...defaultProps} servers={[]} />);
      
      const serverButton = screen.getByLabelText('No servers running');
      expect(serverButton).toBeDisabled();
    });

    it('is enabled when servers are running', () => {
      render(<Header {...defaultProps} servers={mockServers} />);
      
      const serverButton = screen.getByLabelText('2 servers running');
      expect(serverButton).not.toBeDisabled();
    });

    it('shows correct count for single server', () => {
      render(<Header {...defaultProps} servers={[mockServers[0]]} />);
      
      const serverButton = screen.getByLabelText('1 server running');
      expect(serverButton).toBeInTheDocument();
    });

    it('shows badge with server count when servers are running', () => {
      render(<Header {...defaultProps} servers={mockServers} />);
      
      expect(screen.getByText('2')).toBeInTheDocument();
    });

    it('does not show badge when no servers are running', () => {
      render(<Header {...defaultProps} servers={[]} />);
      
      // Badge should not exist
      expect(screen.queryByText('0')).not.toBeInTheDocument();
    });

    it('opens runs popover when clicked with running servers', () => {
      render(<Header {...defaultProps} servers={mockServers} />);
      
      const serverButton = screen.getByLabelText('2 servers running');
      fireEvent.click(serverButton);
      
      expect(screen.getByTestId('runs-popover')).toBeInTheDocument();
    });
  });

  describe('settings button', () => {
    it('calls onOpenSettings when clicked', () => {
      render(<Header {...defaultProps} />);
      
      // Get the desktop nav settings button (first one)
      const settingsButtons = screen.getAllByRole('button', { name: /settings/i });
      fireEvent.click(settingsButtons[0]);
      
      expect(mockOnOpenSettings).toHaveBeenCalledTimes(1);
    });

    it('has correct title attribute', () => {
      render(<Header {...defaultProps} />);
      
      // Get the desktop nav settings button which has the title attribute
      const settingsButton = screen.getByTitle('Open settings');
      expect(settingsButton).toBeInTheDocument();
    });

    it('has correct aria-label', () => {
      render(<Header {...defaultProps} />);
      
      const settingsButton = screen.getByLabelText('Open settings');
      expect(settingsButton).toBeInTheDocument();
    });
  });

  describe('mobile menu', () => {
    it('renders mobile menu toggle button', () => {
      render(<Header {...defaultProps} />);
      
      const menuToggle = screen.getByLabelText('Open menu');
      expect(menuToggle).toBeInTheDocument();
    });

    it('toggles mobile menu when clicked', () => {
      render(<Header {...defaultProps} />);
      
      const menuToggle = screen.getByLabelText('Open menu');
      fireEvent.click(menuToggle);
      
      // After clicking, button should show "Close menu"
      expect(screen.getByLabelText('Close menu')).toBeInTheDocument();
    });

    it('shows server status in mobile menu', () => {
      render(<Header {...defaultProps} servers={mockServers} />);
      
      // Mobile menu item shows server count
      expect(screen.getByText('🖥️ 2 Running')).toBeInTheDocument();
    });

    it('shows "No Servers" in mobile menu when no servers', () => {
      render(<Header {...defaultProps} servers={[]} />);
      
      expect(screen.getByText('🖥️ No Servers')).toBeInTheDocument();
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
      // Server status, Settings, Mobile menu toggle, Mobile menu items
      expect(buttons.length).toBeGreaterThanOrEqual(3);
    });

    it('mobile menu toggle has aria-expanded attribute', () => {
      render(<Header {...defaultProps} />);
      
      const menuToggle = screen.getByLabelText('Open menu');
      expect(menuToggle).toHaveAttribute('aria-expanded', 'false');
      
      fireEvent.click(menuToggle);
      expect(screen.getByLabelText('Close menu')).toHaveAttribute('aria-expanded', 'true');
    });
  });
});
