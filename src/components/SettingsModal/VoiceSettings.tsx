/**
 * VoiceSettings — voice mode configuration panel in the settings modal.
 *
 * Separates model *management* (downloading) from model *selection*
 * (choosing persisted defaults). Default models are saved to the SQLite
 * settings store so voice mode "just works" on subsequent launches
 * without re-visiting settings.
 *
 * @module components/SettingsModal/VoiceSettings
 */

import { FC, useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "../ui/Button";
import { Select } from "../ui/Select";
import { useVoiceMode } from "../../hooks/useVoiceMode";
import { useSettingsContext } from "../../contexts/SettingsContext";
import type { VoiceInteractionMode } from "../../services/clients/voice";
import styles from "../SettingsModal.module.css";

interface VoiceSettingsProps {
  onClose: () => void;
}

export const VoiceSettings: FC<VoiceSettingsProps> = ({ onClose }) => {
  const voice = useVoiceMode();
  const { settings, save: saveSettings } = useSettingsContext();

  // ── Local state initialised from persisted settings ────────────
  const [downloadTarget, setDownloadTarget] = useState<string>('base.en');
  const [selectedVoice, setSelectedVoice] = useState<string>(
    settings?.voiceTtsVoice ?? 'af_sarah',
  );
  const [speedInput, setSpeedInput] = useState<number>(
    settings?.voiceTtsSpeed ?? 1.0,
  );
  const [downloading, setDownloading] = useState<string | null>(null);

  // Derived: the persisted default STT model
  const defaultSttModel = settings?.voiceSttModel ?? null;

  // Re-sync local state when persisted settings load/change
  useEffect(() => {
    if (settings?.voiceTtsVoice) setSelectedVoice(settings.voiceTtsVoice);
    if (settings?.voiceTtsSpeed != null) setSpeedInput(settings.voiceTtsSpeed);
  }, [settings?.voiceTtsVoice, settings?.voiceTtsSpeed]);

  // Load models and devices on mount
  useEffect(() => {
    voice.refreshModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Downloaded model lists (derived from model catalog) ────────
  const downloadedSttIds = useMemo(
    () => new Set(voice.models?.sttDownloaded ?? []),
    [voice.models?.sttDownloaded],
  );
  const ttsDownloaded = voice.models?.ttsDownloaded ?? false;

  // If the persisted default points to a model no longer on disk, clear it
  useEffect(() => {
    if (
      defaultSttModel &&
      voice.models &&               // catalog has loaded
      !downloadedSttIds.has(defaultSttModel)
    ) {
      // Model was deleted from disk — reset persisted default
      saveSettings({ voiceSttModel: null }).catch(() => {});
    }
    // Only run when the download list or default changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [downloadedSttIds, defaultSttModel, voice.models]);

  // ── Handlers ───────────────────────────────────────────────────

  const handleDownloadStt = useCallback(async () => {
    try {
      setDownloading('stt');
      await voice.downloadSttModel(downloadTarget);
      // Refresh model list so downloadedSttIds updates
      await voice.refreshModels();
      // Auto-set as default if no default is configured yet
      if (!defaultSttModel) {
        await saveSettings({ voiceSttModel: downloadTarget });
      }
    } catch {
      // Error handled by the hook
    } finally {
      setDownloading(null);
    }
  }, [voice, downloadTarget, defaultSttModel, saveSettings]);

  const handleDownloadTts = useCallback(async () => {
    try {
      setDownloading('tts');
      await voice.downloadTtsModel();
      // Refresh model list so ttsDownloaded updates
      await voice.refreshModels();
      // Persist default voice if not already set
      if (!settings?.voiceTtsVoice) {
        await saveSettings({ voiceTtsVoice: 'af_sarah' });
      }
    } catch {
      // Error handled by the hook
    } finally {
      setDownloading(null);
    }
  }, [voice, settings?.voiceTtsVoice, saveSettings]);

  const handleDefaultSttChange = useCallback(async (modelId: string) => {
    await saveSettings({ voiceSttModel: modelId });
    // If a pipeline is already alive, hot-swap the STT engine
    if (voice.sttLoaded) {
      voice.loadStt(modelId);
    }
  }, [saveSettings, voice]);

  const handleModeChange = useCallback(async (mode: string) => {
    const m = mode as VoiceInteractionMode;
    voice.setMode(m);
    await saveSettings({ voiceInteractionMode: m });
  }, [voice, saveSettings]);

  const handleVoiceChange = useCallback(async (voiceId: string) => {
    setSelectedVoice(voiceId);
    voice.setVoice(voiceId);
    await saveSettings({ voiceTtsVoice: voiceId });
  }, [voice, saveSettings]);

  const handleSpeedChange = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const speed = parseFloat(e.target.value);
    if (!isNaN(speed)) {
      setSpeedInput(speed);
      voice.setSpeed(speed);
      // Debounce-persist: save only on mouse-up is fine because the range
      // input fires onChange continuously. We persist on every change for
      // simplicity — the settings service merges partial updates.
      await saveSettings({ voiceTtsSpeed: speed });
    }
  }, [voice, saveSettings]);

  const handleAutoSpeakChange = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const enabled = e.target.checked;
    voice.setAutoSpeak(enabled);
    await saveSettings({ voiceAutoSpeak: enabled });
  }, [voice, saveSettings]);

  if (!voice.isSupported) {
    return (
      <div className={styles.section}>
        <p className={styles.description}>
          Voice mode is only available in the desktop application.
        </p>
      </div>
    );
  }

  // ── Render helpers ─────────────────────────────────────────────

  const downloadedSttModels = voice.models?.sttModels?.filter(
    (m) => downloadedSttIds.has(m.id),
  ) ?? [];

  const sttButtonLabel = (() => {
    if (downloading === 'stt') return 'Downloading…';
    if (downloadedSttIds.has(downloadTarget)) return '✓ Downloaded';
    return 'Download';
  })();

  const ttsButtonLabel = (() => {
    if (downloading === 'tts') return 'Downloading…';
    if (ttsDownloaded) return '✓ Downloaded';
    return 'Download';
  })();

  return (
    <div className={styles.form}>
      {/* Error display */}
      {voice.error && (
        <div className={styles.error}>
          {voice.error}
          <button onClick={voice.clearError} style={{ marginLeft: 8, cursor: 'pointer', background: 'none', border: 'none', color: 'inherit' }}>✕</button>
        </div>
      )}

      {/* ── STT Model Section ──────────────────────────────────── */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Speech-to-Text</h3>
        <p className={styles.description}>
          Choose an STT model for speech recognition. Larger models are more
          accurate but slower and use more memory.
        </p>

        {/* Download row: pick any catalog model and download it */}
        <div className={styles.fieldGroup}>
          <label className={styles.label}>Download a Model</label>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <Select
              value={downloadTarget}
              onChange={(e) => setDownloadTarget(e.target.value)}
              style={{ flex: 1 }}
            >
              {voice.models?.sttModels?.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name} ({m.sizeDisplay}) {m.englishOnly ? '🇺🇸' : '🌐'}
                  {downloadedSttIds.has(m.id) ? ' ✓' : ''}
                </option>
              )) ?? <option>Loading...</option>}
            </Select>
            <Button
              onClick={handleDownloadStt}
              disabled={downloading === 'stt' || downloadedSttIds.has(downloadTarget)}
              variant="secondary"
              size="sm"
            >
              {sttButtonLabel}
            </Button>
          </div>
        </div>

        {/* Default model selector: only downloaded models */}
        <div className={styles.fieldGroup}>
          <label className={styles.label}>Default STT Model</label>
          {downloadedSttModels.length > 0 ? (
            <Select
              value={defaultSttModel ?? ''}
              onChange={(e) => handleDefaultSttChange(e.target.value)}
            >
              {!defaultSttModel && (
                <option value="" disabled>Select a default…</option>
              )}
              {downloadedSttModels.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name} ({m.sizeDisplay}) {m.englishOnly ? '🇺🇸' : '🌐'}
                </option>
              ))}
            </Select>
          ) : (
            <p className={styles.description} style={{ fontStyle: 'italic' }}>
              Download a model above to set a default.
            </p>
          )}
        </div>
      </div>

      {/* ── TTS Section ────────────────────────────────────────── */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Text-to-Speech</h3>
        <p className={styles.description}>
          High-quality local TTS. The model will be downloaded on first use (~300 MB).
        </p>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>TTS Model</label>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <span style={{ flex: 1, fontSize: '0.85rem', color: 'var(--color-text-secondary)' }}>
              {voice.models?.ttsModel?.name ?? 'TTS Model'}
            </span>
            <Button
              onClick={handleDownloadTts}
              disabled={downloading === 'tts' || ttsDownloaded}
              variant="secondary"
              size="sm"
            >
              {ttsButtonLabel}
            </Button>
          </div>
        </div>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>Default Voice</label>
          <Select
            value={selectedVoice}
            onChange={(e) => handleVoiceChange(e.target.value)}
          >
            {voice.models?.voices?.map((v) => (
              <option key={v.id} value={v.id}>
                {v.name} ({v.category})
              </option>
            )) ?? <option>Loading...</option>}
          </Select>
        </div>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>Speed ({speedInput.toFixed(1)}x)</label>
          <input
            type="range"
            min="0.5"
            max="2.0"
            step="0.1"
            value={speedInput}
            onChange={handleSpeedChange}
            style={{ width: '100%' }}
          />
        </div>
      </div>

      {/* ── Interaction Mode ───────────────────────────────────── */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Interaction Mode</h3>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>Mode</label>
          <Select
            value={voice.mode}
            onChange={(e) => handleModeChange(e.target.value)}
          >
            <option value="ptt">Push to Talk (Space bar)</option>
            <option value="vad">Voice Activity Detection (hands-free)</option>
          </Select>
        </div>

        <div className={styles.fieldGroup}>
          <label className={styles.label} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <input
              type="checkbox"
              checked={voice.autoSpeak}
              onChange={handleAutoSpeakChange}
            />
            Auto-speak responses
          </label>
          <p className={styles.description}>
            Automatically read LLM responses aloud using TTS.
          </p>
        </div>
      </div>

      {/* ── Audio Devices ──────────────────────────────────────── */}
      {voice.devices.length > 0 && (
        <div className={styles.section}>
          <h3 className={styles.sectionTitle}>Audio Devices</h3>
          <div className={styles.fieldGroup}>
            <label className={styles.label}>Input Device</label>
            <Select value="" onChange={() => {}}>
              {voice.devices.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name} {d.isDefault ? '(default)' : ''}
                </option>
              ))}
            </Select>
          </div>
        </div>
      )}

      {/* ── Download Progress ──────────────────────────────────── */}
      {voice.downloadProgress && (
        <div className={styles.section}>
          <p className={styles.description}>
            Downloading {voice.downloadProgress.modelId}…{' '}
            {voice.downloadProgress.percent.toFixed(0)}%
          </p>
          <div style={{
            width: '100%',
            height: 4,
            background: 'var(--color-border)',
            borderRadius: 2,
            overflow: 'hidden',
          }}>
            <div style={{
              width: `${voice.downloadProgress.percent}%`,
              height: '100%',
              background: 'var(--color-accent)',
              borderRadius: 2,
              transition: 'width 200ms ease',
            }} />
          </div>
        </div>
      )}

      {/* ── Status ─────────────────────────────────────────────── */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Status</h3>
        <div style={{ fontSize: '0.85rem', color: 'var(--color-text-secondary)' }}>
          <div>Pipeline: {voice.isActive ? '🟢 Active' : '⚪ Inactive'}</div>
          <div>STT Engine: {voice.sttLoaded ? '✓ Loaded' : '✗ Not loaded'}</div>
          <div>TTS Engine: {voice.ttsLoaded ? '✓ Loaded' : '✗ Not loaded'}</div>
          <div>Default STT: {defaultSttModel ?? '—'}</div>
          <div>Default Voice: {selectedVoice}</div>
          <div>State: {voice.voiceState}</div>
        </div>
      </div>

      {/* ── Actions ────────────────────────────────────────────── */}
      <div className={styles.actions}>
        <Button onClick={onClose} variant="secondary" size="sm">
          Close
        </Button>
      </div>
    </div>
  );
};
