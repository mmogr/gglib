import { FC } from 'react';

interface InspectorActionsProps {
  isRunning: boolean;
  isEditMode: boolean;
  onToggleServer: () => void;
  onEdit: () => void;
  onSave: () => void;
  onCancel: () => void;
  onDelete: () => void;
}

/**
 * Action button row for the model inspector.
 * Shows Start/Stop Endpoint, Edit/Save/Cancel, and Delete buttons.
 */
export const InspectorActions: FC<InspectorActionsProps> = ({
  isRunning,
  isEditMode,
  onToggleServer,
  onEdit,
  onSave,
  onCancel,
  onDelete,
}) => {
  return (
    <section className="inspector-section actions-section">
      <button 
        className={`btn btn-lg ${isRunning ? 'btn-danger' : 'btn-primary'}`}
        onClick={onToggleServer}
        disabled={isEditMode}
      >
        {isRunning ? '⏹️ Stop Endpoint' : '🚀 Start Endpoint'}
      </button>
      <div className="secondary-actions">
        {isEditMode ? (
          <>
            <button className="btn btn-primary" onClick={onSave}>
              ✓ Save
            </button>
            <button className="btn btn-secondary" onClick={onCancel}>
              ✕ Cancel
            </button>
          </>
        ) : (
          <>
            <button className="btn btn-secondary" onClick={onEdit}>
              ✏️ Edit
            </button>
            <button className="btn btn-secondary" onClick={onDelete}>
              🗑️ Delete
            </button>
          </>
        )}
      </div>
    </section>
  );
};
