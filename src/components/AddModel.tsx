import { useState, FC, FormEvent } from "react";
import { TauriService } from "../services/tauri";
import styles from './AddModel.module.css';

// Check if we're in Tauri environment
const isTauri = typeof (window as any).__TAURI_INTERNALS__ !== 'undefined';

interface AddModelProps {
  onModelAdded: () => void;
}

const AddModel: FC<AddModelProps> = ({ onModelAdded }) => {
  const [filePath, setFilePath] = useState("");
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleBrowse = async () => {
    if (!isTauri) {
      // In web mode, use standard file input
      const input = document.createElement('input');
      input.type = 'file';
      input.accept = '.gguf';
      input.onchange = (e) => {
        const file = (e.target as HTMLInputElement).files?.[0];
        if (file) {
          setFilePath(file.name);
          setError("Note: In web mode, you'll need to provide the full server path");
        }
      };
      input.click();
      return;
    }

    try {
      // Dynamic import for Tauri dialog
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{
          name: 'GGUF Models',
          extensions: ['gguf']
        }]
      });
      
      if (selected) {
        setFilePath(selected);
      }
    } catch (err) {
      console.error("Failed to open file dialog:", err);
      setError("File browser not available. Please enter the path manually.");
    }
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    
    if (!filePath.trim()) {
      setError("Please provide a file path");
      return;
    }

    try {
      setAdding(true);
      setError(null);
      await TauriService.addModel(filePath.trim());
      setFilePath("");
      onModelAdded();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add model");
    } finally {
      setAdding(false);
    }
  };

  return (
    <div className="add-model-container">
      <h2>Add New Model</h2>

      <form onSubmit={handleSubmit} className="add-model-form">
        <div className="form-group">
          <label htmlFor="filePath">Model File Path:</label>
          <div className={styles.fileInputGroup}>
            <input
              type="text"
              id="filePath"
              value={filePath}
              onChange={(e) => setFilePath(e.target.value)}
              placeholder="/path/to/your/model.gguf"
              className={`form-input ${styles.formInput}`}
            />
            <button
              type="button"
              onClick={handleBrowse}
              className={styles.browseButton}
            >
              📁 Browse
            </button>
          </div>
        </div>

        {error && <div className="error-message">{error}</div>}

        <div className="form-actions">
          <button
            type="submit"
            disabled={adding || !filePath.trim()}
            className="btn btn-primary"
          >
            {adding ? "Adding Model..." : "Add Model"}
          </button>
        </div>
      </form>
    </div>
  );
};

export default AddModel;