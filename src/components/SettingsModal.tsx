import { FC, FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { useModelsDirectory } from "../hooks/useModelsDirectory";
import { useSettings } from "../hooks/useSettings";
import { UpdateSettingsRequest } from "../types";
import styles from "./SettingsModal.module.css";

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const sourceLabels: Record<string, string> = {
  explicit: "Custom path (CLI/UI override)",
  env: "Configured via .env",
  default: "Default (~/.local/share/llama_models)",
};

export const SettingsModal: FC<SettingsModalProps> = ({ isOpen, onClose }) => {
  const { info, loading: loadingDir, saving: savingDir, error: dirError, refresh: refreshDir, save: saveDir } = useModelsDirectory();
  const { settings, loading: loadingSettings, saving: savingSettings, error: settingsError, refresh: refreshSettings, save: saveSettings } = useSettings();
  
  const [pathInput, setPathInput] = useState("");
  const [contextSizeInput, setContextSizeInput] = useState("");
  const [proxyPortInput, setProxyPortInput] = useState("");
  const [serverPortInput, setServerPortInput] = useState("");
  const [successMessage, setSuccessMessage] = useState<string | null>(null);

  const loading = loadingDir || loadingSettings;
  const saving = savingDir || savingSettings;
  const error = dirError || settingsError;

  useEffect(() => {
    if (info?.path) {
      setPathInput(info.path);
    }
  }, [info]);

  useEffect(() => {
    if (settings) {
      setContextSizeInput(settings.default_context_size?.toString() || "");
      setProxyPortInput(settings.proxy_port?.toString() || "");
      setServerPortInput(settings.server_port?.toString() || "");
    }
  }, [settings]);

  const handleOverlayClick = useCallback(() => {
    if (!saving) {
      onClose();
    }
  }, [onClose, saving]);

  const handleSubmit = useCallback(
    async (event: FormEvent) => {
      event.preventDefault();
      setSuccessMessage(null);
      
      try {
        // Update models directory if changed
        if (pathInput.trim() && pathInput !== info?.path) {
          await saveDir(pathInput.trim());
        }

        // Helper function to parse numeric input
        const parseNumericInput = (input: string): number | null => {
          if (!input.trim()) return null;
          const parsed = parseInt(input.trim(), 10);
          return isNaN(parsed) ? null : parsed;
        };

        // Update other settings
        const updates: UpdateSettingsRequest = {
          default_context_size: parseNumericInput(contextSizeInput),
          proxy_port: parseNumericInput(proxyPortInput),
          server_port: parseNumericInput(serverPortInput),
        };

        // Check if any updates were made
        const hasUpdates = 
          updates.default_context_size !== undefined ||
          updates.proxy_port !== undefined ||
          updates.server_port !== undefined;

        if (hasUpdates) {
          await saveSettings(updates);
        }

        setSuccessMessage("Settings updated successfully");
      } catch (err) {
        console.error("Failed to update settings", err);
      }
    },
    [pathInput, contextSizeInput, proxyPortInput, serverPortInput, info, saveDir, saveSettings]
  );

  const handleReset = useCallback(() => {
    if (info?.default_path) {
      setPathInput(info.default_path);
    }
    if (settings) {
      setContextSizeInput(settings.default_context_size?.toString() || "4096");
      setProxyPortInput(settings.proxy_port?.toString() || "8080");
      setServerPortInput(settings.server_port?.toString() || "9000");
    }
  }, [info, settings]);

  const handleRefresh = useCallback(() => {
    refreshDir();
    refreshSettings();
  }, [refreshDir, refreshSettings]);

  const sourceDescription = useMemo(() => {
    if (!info) {
      return null;
    }
    return sourceLabels[info.source] || info.source;
  }, [info]);

  if (!isOpen) {
    return null;
  }

  return (
    <div className="modal-overlay" onClick={handleOverlayClick}>
      <div className="modal modal-md" onClick={(event) => event.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">Settings</h2>
          <button className="modal-close" onClick={handleOverlayClick} aria-label="Close settings dialog">
            ×
          </button>
        </div>
        <div className="modal-body">
          {loading && (
            <div className="modal-loading">
              <div className="modal-spinner" aria-hidden />
              <p className="modal-loading-text">Loading current settings…</p>
            </div>
          )}

          {!loading && (
            <form className={styles.form} onSubmit={handleSubmit}>
              <label className={styles.label} htmlFor="models-dir-input">
                Default Download Path
              </label>
              <input
                id="models-dir-input"
                className={styles.input}
                value={pathInput}
                onChange={(event) => setPathInput(event.target.value)}
                placeholder="/path/to/models"
                disabled={saving}
              />
              <div className={styles.helperText}>
                {sourceDescription && <span>{sourceDescription}</span>}
                {info?.default_path && (
                  <button type="button" className={styles.resetLink} onClick={handleReset}>
                    Reset to defaults
                  </button>
                )}
              </div>

              {info && (
                <div className={styles.statusChips} role="status" aria-live="polite">
                  <span
                    className={`${styles.chip} ${info.exists ? styles.chipOk : styles.chipWarn}`}
                    aria-label={info.exists ? "Directory exists" : "Directory will be created (warning)"}
                  >
                    {info.exists ? "Directory exists" : "Directory will be created"}
                  </span>
                  <span
                    className={`${styles.chip} ${info.writable ? styles.chipOk : styles.chipError}`}
                    aria-label={info.writable ? "Writable" : "Not writable (error)"}
                  >
                    {info.writable ? "Writable" : "Not writable"}
                  </span>
                </div>
              )}

              <div className={styles.separator} />

              <label className={styles.label} htmlFor="context-size-input">
                Default Context Size
              </label>
              <input
                id="context-size-input"
                type="number"
                className={styles.input}
                value={contextSizeInput}
                onChange={(event) => setContextSizeInput(event.target.value)}
                placeholder="4096"
                min="512"
                max="1000000"
                disabled={saving}
              />
              <div className={styles.helperText}>
                <span>Default context size for models (e.g., 4096, 8192, 16384)</span>
              </div>

              <label className={styles.label} htmlFor="proxy-port-input">
                Proxy Server Port
              </label>
              <input
                id="proxy-port-input"
                type="number"
                className={styles.input}
                value={proxyPortInput}
                onChange={(event) => setProxyPortInput(event.target.value)}
                placeholder="8080"
                min="1024"
                max="65535"
                disabled={saving}
              />
              <div className={styles.helperText}>
                <span>Port for the OpenAI-compatible proxy server</span>
              </div>

              <label className={styles.label} htmlFor="server-port-input">
                Base Server Port
              </label>
              <input
                id="server-port-input"
                type="number"
                className={styles.input}
                value={serverPortInput}
                onChange={(event) => setServerPortInput(event.target.value)}
                placeholder="9000"
                min="1024"
                max="65535"
                disabled={saving}
              />
              <div className={styles.helperText}>
                <span>Starting port for llama-server instances</span>
              </div>

              {error && <p className={styles.error} role="alert">{error}</p>}
              {successMessage && <p className={styles.success} role="status" aria-live="polite">{successMessage}</p>}

              <div className="modal-footer modal-footer-between">
                <button type="button" className="btn btn-secondary" onClick={handleRefresh} disabled={loading || saving}>
                  Refresh
                </button>
                <div className={styles.footerActions}>
                  <button type="button" className="btn btn-secondary" onClick={handleOverlayClick} disabled={saving}>
                    Cancel
                  </button>
                  <button type="submit" className="btn btn-primary" disabled={saving}>
                    {saving ? "Saving…" : "Save changes"}
                  </button>
                </div>
              </div>
            </form>
          )}
        </div>
      </div>
    </div>
  );
};

export default SettingsModal;
