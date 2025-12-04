import { FC, FormEvent } from "react";

/** Common quantization options for the datalist */
const COMMON_QUANTIZATIONS = [
  "Q4_0", "Q4_1", "Q5_0", "Q5_1", "Q8_0",
  "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L",
  "Q4_K_S", "Q4_K_M", "Q5_K_S", "Q5_K_M",
  "Q6_K", "Q8_K"
];

interface DownloadFormProps {
  repoId: string;
  setRepoId: (value: string) => void;
  quantization: string;
  setQuantization: (value: string) => void;
  submitting: boolean;
  canSubmit: boolean;
  isDownloading: boolean;
  error: string | null;
  onSubmit: (e: FormEvent) => Promise<void>;
}

/**
 * Presentational form for downloading models from HuggingFace.
 * Displays repo ID input, quantization selector, and submit button.
 */
const DownloadForm: FC<DownloadFormProps> = ({
  repoId,
  setRepoId,
  quantization,
  setQuantization,
  submitting,
  canSubmit,
  isDownloading,
  error,
  onSubmit,
}) => {
  return (
    <form onSubmit={onSubmit} className="download-form">
      <div className="form-group">
        <label htmlFor="repoId">Repository ID:</label>
        <input
          type="text"
          id="repoId"
          value={repoId}
          onChange={(e) => setRepoId(e.target.value)}
          placeholder="e.g. microsoft/DialoGPT-medium"
          className="form-input"
          required
          disabled={submitting}
        />
        <small className="form-hint">
          Format: username/repository-name or organization/repository-name
        </small>
      </div>

      <div className="form-group">
        <label htmlFor="quantization">Quantization (optional):</label>
        <input
          type="text"
          id="quantization"
          list="quantization-options"
          value={quantization}
          onChange={(e) => setQuantization(e.target.value)}
          placeholder="Auto-detect or enter custom (e.g., Q4_K_M)"
          className="form-input"
          disabled={submitting}
        />
        <datalist id="quantization-options">
          {COMMON_QUANTIZATIONS.map((quant) => (
            <option key={quant} value={quant} />
          ))}
        </datalist>
        <small className="form-hint">
          Select from common options or type your own quantization format
        </small>
      </div>

      {error && <div className="error-message">{error}</div>}

      <div className="form-actions">
        <button
          type="submit"
          disabled={!canSubmit}
          className="btn btn-primary"
        >
          {submitting ? "Adding to Queue..." : isDownloading ? "Add to Queue" : "Download Model"}
        </button>
      </div>
    </form>
  );
};

export default DownloadForm;
