import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import Sidebar from '../../../src/components/Sidebar';

describe('Sidebar', () => {
  const mockOnViewChange = vi.fn();

  beforeEach(() => {
    mockOnViewChange.mockClear();
  });

  describe('rendering', () => {
    it('renders all navigation items', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      // Check for buttons by their accessible name (icon + text)
      expect(screen.getByRole('button', { name: /📋 Models/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /➕ Model/i })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /⬇️ Download/i })).toBeInTheDocument();
    });

    it('renders the section title', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      // Section title "Models" is in the nav section - there are two "Models" texts
      const allModels = screen.getAllByText('Models');
      expect(allModels.length).toBeGreaterThanOrEqual(1);
    });

    it('highlights the current view', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      const modelsButton = screen.getByRole('button', { name: /📋 Models/i });
      expect(modelsButton).toHaveClass('active');
    });

    it('highlights add-model when it is current view', () => {
      render(<Sidebar currentView="add-model" onViewChange={mockOnViewChange} />);
      
      const addModelButton = screen.getByRole('button', { name: /➕ Model/i });
      expect(addModelButton).toHaveClass('active');
    });

    it('highlights download when it is current view', () => {
      render(<Sidebar currentView="download" onViewChange={mockOnViewChange} />);
      
      const downloadButton = screen.getByRole('button', { name: /Download/i });
      expect(downloadButton).toHaveClass('active');
    });
  });

  describe('navigation', () => {
    it('calls onViewChange when clicking Models', () => {
      render(<Sidebar currentView="add-model" onViewChange={mockOnViewChange} />);
      
      const modelsButton = screen.getByRole('button', { name: /📋 Models/i });
      fireEvent.click(modelsButton);
      
      expect(mockOnViewChange).toHaveBeenCalledWith('models');
    });

    it('calls onViewChange when clicking Add Model', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      const addModelButton = screen.getByRole('button', { name: /➕ Model/i });
      fireEvent.click(addModelButton);
      
      expect(mockOnViewChange).toHaveBeenCalledWith('add-model');
    });

    it('calls onViewChange when clicking Download', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      const downloadButton = screen.getByRole('button', { name: /⬇️ Download/i });
      fireEvent.click(downloadButton);
      
      expect(mockOnViewChange).toHaveBeenCalledWith('download');
    });

    it('calls onViewChange even when clicking the already active item', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      const modelsButton = screen.getByRole('button', { name: /📋 Models/i });
      fireEvent.click(modelsButton);
      
      expect(mockOnViewChange).toHaveBeenCalledWith('models');
    });
  });

  describe('accessibility', () => {
    it('renders buttons as accessible elements', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      const buttons = screen.getAllByRole('button');
      expect(buttons).toHaveLength(3);
    });

    it('renders navigation with proper structure', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      const nav = screen.getByRole('navigation');
      expect(nav).toBeInTheDocument();
    });

    it('renders list items for each nav item', () => {
      render(<Sidebar currentView="models" onViewChange={mockOnViewChange} />);
      
      const list = screen.getByRole('list');
      expect(list).toBeInTheDocument();
      expect(list.querySelectorAll('li')).toHaveLength(3);
    });
  });
});
