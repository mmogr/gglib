/**
 * VoiceSettings — voice mode configuration panel in the settings modal.
 *
 * Controls STT/TTS model selection, interaction mode, voice choice,
 * speed, VAD sensitivity, and model download.
 */

import { FC, useCallback, useEffect, useState } from "react";
import { Button } from "../ui/Button";
import { Select } from "../ui/Select";
import { useVoiceMode } from "../../hooks/useVoiceMode";
import type { VoiceInteractionMode } from "../../services/clients/voice";
import styles from "../SettingsModal.module.css";

interface VoiceSettingsProps {
  onClose: () => void;
}

export const VoiceSettings: FC<VoiceSettingsProps> = ({ onClose }) => {
  const voice = useVoiceMode();
  const [selectedSttModel, setSelectedSttModel] = useState<string>('whisper-base');
  const [selectedVoice, setSelectedVoice] = useState<string>('af_sarah');
  const [speedInput, setSpeedInput] = useState<number>(1.0);
  const [downloading, setDownloading] = useState<string | null>(null);

  // Load models and devices on mount
  useEffect(() => {
    voice.refreshModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleDownloadStt = useCallback(async () => {
    try {
      setDownloading('stt');
      const path = await voice.downloadSttModel(selectedSttModel);
      await voice.loadStt(path);
    } catch {
      // Error handled by the hook
    } finally {
      setDownloading(null);
    }
  }, [voice, selectedSttModel]);

  const handleDownloadTts = useCallback(async () => {
    try {
      setDownloading('tts');
      const path = await voice.downloadTtsModel();
      await voice.loadTts(path);
    } catch {
      // Error handled by the hook
    } finally {
      setDownloading(null);
    }
  }, [voice]);

  const handleModeChange = useCallback((mode: string) => {
    voice.setMode(mode as VoiceInteractionMode);
  }, [voice]);

  const handleVoiceChange = useCallback((voiceId: string) => {
    setSelectedVoice(voiceId);
    voice.setVoice(voiceId);
  }, [voice]);

  const handleSpeedChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const speed = parseFloat(e.target.value);
    if (!isNaN(speed)) {
      setSpeedInput(speed);
      voice.setSpeed(speed);
    }
  }, [voice]);

  const handleAutoSpeakChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    voice.setAutoSpeak(e.target.checked);
  }, [voice]);

  if (!voice.isSupported) {
    return (
      <div className={styles.section}>
        <p className={styles.description}>
          Voice mode is only available in the desktop application.
        </p>
      </div>
    );
  }

  return (
    <div className={styles.form}>
      {/* Error display */}
      {voice.error && (
        <div className={styles.error}>
          {voice.error}
          <button onClick={voice.clearError} style={{ marginLeft: 8, cursor: 'pointer', background: 'none', border: 'none', color: 'inherit' }}>✕</button>
        </div>
      )}

      {/* STT Model Section */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Speech-to-Text (Whisper)</h3>
        <p className={styles.description}>
          Choose a Whisper model for speech recognition. Larger models are more
          accurate but slower and use more memory.
        </p>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>STT Model</label>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <Select
              value={selectedSttModel}
              onChange={(e) => setSelectedSttModel(e.target.value)}
              style={{ flex: 1 }}
            >
              {voice.models?.sttModels?.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name} ({m.size}) {m.isQuantized ? '⚡' : ''}
                </option>
              )) ?? <option>Loading...</option>}
            </Select>
            <Button
              onClick={handleDownloadStt}
              disabled={downloading === 'stt'}
              variant="secondary"
              size="sm"
            >
              {downloading === 'stt' ? 'Downloading…' : voice.sttLoaded ? '✓ Loaded' : 'Download & Load'}
            </Button>
          </div>
        </div>
      </div>

      {/* TTS Section */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Text-to-Speech (Kokoro)</h3>
        <p className={styles.description}>
          High-quality local TTS. The model will be downloaded on first use (~300 MB).
        </p>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>TTS Model</label>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <span style={{ flex: 1, fontSize: '0.85rem', color: 'var(--color-text-secondary)' }}>
              {voice.models?.ttsModel?.modelName ?? 'Kokoro TTS'}
            </span>
            <Button
              onClick={handleDownloadTts}
              disabled={downloading === 'tts'}
              variant="secondary"
              size="sm"
            >
              {downloading === 'tts' ? 'Downloading…' : voice.ttsLoaded ? '✓ Loaded' : 'Download & Load'}
            </Button>
          </div>
        </div>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>Voice</label>
          <Select
            value={selectedVoice}
            onChange={(e) => handleVoiceChange(e.target.value)}
          >
            {voice.models?.availableVoices?.map((v) => (
              <option key={v.id} value={v.id}>
                {v.name} ({v.accent}, {v.gender})
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

      {/* Interaction Mode */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Interaction Mode</h3>

        <div className={styles.fieldGroup}>
          <label className={styles.label}>Mode</label>
          <Select
            value={voice.mode}
            onChange={(e) => handleModeChange(e.target.value)}
          >
            <option value="push_to_talk">Push to Talk (Space bar)</option>
            <option value="voice_activity_detection">Voice Activity Detection (hands-free)</option>
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

      {/* Audio Devices */}
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

      {/* Download Progress */}
      {voice.downloadProgress && (
        <div className={styles.section}>
          <p className={styles.description}>
            Downloading {voice.downloadProgress.modelId}…{' '}
            {(voice.downloadProgress.progress * 100).toFixed(0)}%
          </p>
          <div style={{
            width: '100%',
            height: 4,
            background: 'var(--color-border)',
            borderRadius: 2,
            overflow: 'hidden',
          }}>
            <div style={{
              width: `${voice.downloadProgress.progress * 100}%`,
              height: '100%',
              background: 'var(--color-accent)',
              borderRadius: 2,
              transition: 'width 200ms ease',
            }} />
          </div>
        </div>
      )}

      {/* Status */}
      <div className={styles.section}>
        <h3 className={styles.sectionTitle}>Status</h3>
        <div style={{ fontSize: '0.85rem', color: 'var(--color-text-secondary)' }}>
          <div>Pipeline: {voice.isActive ? '🟢 Active' : '⚪ Inactive'}</div>
          <div>STT Engine: {voice.sttLoaded ? '✓ Loaded' : '✗ Not loaded'}</div>
          <div>TTS Engine: {voice.ttsLoaded ? '✓ Loaded' : '✗ Not loaded'}</div>
          <div>State: {voice.voiceState}</div>
        </div>
      </div>

      {/* Actions */}
      <div className={styles.actions}>
        <Button onClick={onClose} variant="secondary" size="sm">
          Close
        </Button>
      </div>
    </div>
  );
};
