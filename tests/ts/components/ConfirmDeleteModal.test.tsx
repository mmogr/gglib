import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ConfirmDeleteModal } from '../../../src/components/ChatMessagesPanel/ConfirmDeleteModal';

describe('ConfirmDeleteModal', () => {
  const mockOnConfirm = vi.fn();
  const mockOnCancel = vi.fn();

  const defaultProps = {
    isOpen: true,
    messageCount: 1,
    isDeleting: false,
    onConfirm: mockOnConfirm,
    onCancel: mockOnCancel,
  };

  beforeEach(() => {
    mockOnConfirm.mockClear();
    mockOnCancel.mockClear();
  });

  describe('rendering', () => {
    it('renders nothing when isOpen is false', () => {
      const { container } = render(
        <ConfirmDeleteModal {...defaultProps} isOpen={false} />
      );
      
      expect(container.firstChild).toBeNull();
    });

    it('renders the modal when isOpen is true', () => {
      render(<ConfirmDeleteModal {...defaultProps} />);
      
      expect(screen.getByText('Delete Message?')).toBeInTheDocument();
    });

    it('renders the delete icon', () => {
      render(<ConfirmDeleteModal {...defaultProps} />);
      
      expect(screen.getByText('🗑️')).toBeInTheDocument();
    });

    it('renders the description text', () => {
      render(<ConfirmDeleteModal {...defaultProps} />);
      
      expect(screen.getByText('This will permanently delete this message.')).toBeInTheDocument();
    });

    it('renders Cancel and Delete buttons', () => {
      render(<ConfirmDeleteModal {...defaultProps} />);
      
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument();
    });
  });

  describe('cascade warning', () => {
    it('does not show warning when messageCount is 1', () => {
      render(<ConfirmDeleteModal {...defaultProps} messageCount={1} />);
      
      expect(screen.queryByText(/subsequent/)).not.toBeInTheDocument();
    });

    it('shows warning when messageCount is greater than 1', () => {
      render(<ConfirmDeleteModal {...defaultProps} messageCount={3} />);
      
      expect(screen.getByText(/This will also delete/)).toBeInTheDocument();
      expect(screen.getByText('2')).toBeInTheDocument();
    });

    it('uses singular "message" when only 1 subsequent message', () => {
      render(<ConfirmDeleteModal {...defaultProps} messageCount={2} />);
      
      expect(screen.getByText(/subsequent message to maintain/)).toBeInTheDocument();
    });

    it('uses plural "messages" when multiple subsequent messages', () => {
      render(<ConfirmDeleteModal {...defaultProps} messageCount={4} />);
      
      expect(screen.getByText(/subsequent messages to maintain/)).toBeInTheDocument();
    });
  });

  describe('button interactions', () => {
    it('calls onCancel when Cancel button is clicked', () => {
      render(<ConfirmDeleteModal {...defaultProps} />);
      
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
      
      expect(mockOnCancel).toHaveBeenCalledTimes(1);
    });

    it('calls onConfirm when Delete button is clicked', () => {
      render(<ConfirmDeleteModal {...defaultProps} />);
      
      fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
      
      expect(mockOnConfirm).toHaveBeenCalledTimes(1);
    });

    it('calls onCancel when clicking overlay', () => {
      render(<ConfirmDeleteModal {...defaultProps} />);
      
      // Get the overlay element (first div rendered by the component)
      const overlay = document.querySelector('[class*="overlay"]');
      if (overlay) {
        fireEvent.click(overlay);
        expect(mockOnCancel).toHaveBeenCalledTimes(1);
      }
    });
  });

  describe('deleting state', () => {
    it('shows "Deleting..." text when isDeleting is true', () => {
      render(<ConfirmDeleteModal {...defaultProps} isDeleting={true} />);
      
      expect(screen.getByText('Deleting...')).toBeInTheDocument();
    });

    it('disables Cancel button when isDeleting is true', () => {
      render(<ConfirmDeleteModal {...defaultProps} isDeleting={true} />);
      
      expect(screen.getByRole('button', { name: 'Cancel' })).toBeDisabled();
    });

    it('disables Delete button when isDeleting is true', () => {
      render(<ConfirmDeleteModal {...defaultProps} isDeleting={true} />);
      
      expect(screen.getByRole('button', { name: 'Deleting...' })).toBeDisabled();
    });

    it('does not call onCancel when clicking overlay while deleting', () => {
      render(<ConfirmDeleteModal {...defaultProps} isDeleting={true} />);
      
      const overlay = document.querySelector('[class*="overlay"]');
      if (overlay) {
        fireEvent.click(overlay);
        expect(mockOnCancel).not.toHaveBeenCalled();
      }
    });
  });
});
