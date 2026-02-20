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
import { useVoiceModeContext } from "../../contexts/VoiceModeContext";
import { useSettingsContext } from "../../contexts/SettingsContext";
import type { VoiceInteractionMode } from "../../services/clients/voice";

interface VoiceSettingsProps {
  onClose: () => void;
}

export const VoiceSettings: FC<VoiceSettingsProps> = ({ onClose }) => {
  const voice = useVoiceModeContext();
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

  // Load models and devices on mount (re-runs if the voice context instance changes).
  useEffect(() => {
    void voice?.refreshModels();
  }, [voice]);

  // ── Downloaded model lists (derived from model catalog) ────────
  const downloadedSttIds = useMemo(
    () => new Set(voice?.models?.sttDownloaded ?? []),
    [voice?.models?.sttDownloaded],
  );
  const ttsDownloaded = voice?.models?.ttsDownloaded ?? false;
  const vadDownloaded = voice?.models?.vadDownloaded ?? false;

  // If the persisted default points to a model no longer on disk, clear it
  useEffect(() => {
    if (
      defaultSttModel &&
      voice?.models &&               // catalog has loaded
      !downloadedSttIds.has(defaultSttModel)
    ) {
      // Model was deleted from disk — reset persisted default
      saveSettings({ voiceSttModel: null }).catch(() => {});
    }
  }, [downloadedSttIds, defaultSttModel, voice?.models, saveSettings]);

  // ── Handlers ───────────────────────────────────────────────────

  const handleDownloadStt = useCallback(async () => {
    try {
      setDownloading('stt');
      await voice?.downloadSttModel(downloadTarget);
      // Refresh model list so downloadedSttIds updates
      await voice?.refreshModels();
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
      await voice?.downloadTtsModel();
      // Refresh model list so ttsDownloaded updates
      await voice?.refreshModels();
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

  const handleDownloadVad = useCallback(async () => {
    try {
      setDownloading('vad');
      await voice?.downloadVadModel();
      await voice?.refreshModels();
    } catch {
      // Error handled by the hook
    } finally {
      setDownloading(null);
    }
  }, [voice]);

  const handleDefaultSttChange = useCallback(async (modelId: string) => {
    await saveSettings({ voiceSttModel: modelId });
    // If a pipeline is already alive, hot-swap the STT engine
    if (voice?.sttLoaded) {
      voice?.loadStt(modelId);
    }
  }, [saveSettings, voice]);

  const handleModeChange = useCallback(async (mode: string) => {
    const m = mode as VoiceInteractionMode;
    voice?.setMode(m);
    await saveSettings({ voiceInteractionMode: m });
  }, [voice, saveSettings]);

  const handleVoiceChange = useCallback(async (voiceId: string) => {
    setSelectedVoice(voiceId);
    voice?.setVoice(voiceId);
    await saveSettings({ voiceTtsVoice: voiceId });
  }, [voice, saveSettings]);

  const handleSpeedChange = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const speed = parseFloat(e.target.value);
    if (!isNaN(speed)) {
      setSpeedInput(speed);
      voice?.setSpeed(speed);
      await saveSettings({ voiceTtsSpeed: speed });
    }
  }, [voice, saveSettings]);

  const handleAutoSpeakChange = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const enabled = e.target.checked;
    voice?.setAutoSpeak(enabled);
    await saveSettings({ voiceAutoSpeak: enabled });
  }, [voice, saveSettings]);

  if (!voice || !voice.isSupported) {
    return (
      <div className="flex flex-col gap-xs">
        <p className="text-sm text-text-secondary">
          Voice mode is only available in the desktop application.
        </p>
      </div>
    );
  }

  // ── Render helpers ─────────────────────────────────────────────

  const downloadedSttModels = voice?.models?.sttModels?.filter(
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
    <div className="flex flex-col gap-md">
      {/* Error display */}
      {voice?.error && (
        <div className="text-[#ef4444] text-sm">
          {voice.error}
          <button onClick={() => voice?.clearError()} className="ml-2 cursor-pointer bg-transparent border-none text-inherit">✕</button>
        </div>
      )}

      {/* ── STT Model Section ──────────────────────────────────── */}
      <div className="flex flex-col gap-xs">
        <h3 className="font-semibold text-text">Speech-to-Text</h3>
        <p className="text-sm text-text-secondary">
          Choose an STT model for speech recognition. Larger models are more
          accurate but slower and use more memory.
        </p>

        {/* Download row: pick any catalog model and download it */}
        <div className="flex flex-col gap-xs">
          <label className="font-semibold text-text">Download a Model</label>
          <div className="flex gap-2 items-center">
            <Select
              value={downloadTarget}
              onChange={(e) => setDownloadTarget(e.target.value)}
              className="flex-1"
            >
              {voice?.models?.sttModels?.map((m) => (
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
        <div className="flex flex-col gap-xs">
          <label className="font-semibold text-text">Default STT Model</label>
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
            <p className="text-sm text-text-secondary italic">
              Download a model above to set a default.
            </p>
          )}
        </div>
      </div>

      {/* ── TTS Section ────────────────────────────────────────── */}
      <div className="flex flex-col gap-xs">
        <h3 className="font-semibold text-text">Text-to-Speech</h3>
        <p className="text-sm text-text-secondary">
          High-quality local TTS. The model will be downloaded on first use (~300 MB).
        </p>

        <div className="flex flex-col gap-xs">
          <label className="font-semibold text-text">TTS Model</label>
          <div className="flex gap-2 items-center">
            <span className="flex-1 text-sm text-text-secondary">
              {voice?.models?.ttsModel?.name ?? 'TTS Model'}
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

        <div className="flex flex-col gap-xs">
          <label className="font-semibold text-text">Default Voice</label>
          <Select
            value={selectedVoice}
            onChange={(e) => handleVoiceChange(e.target.value)}
          >
            {voice?.models?.voices?.map((v) => (
              <option key={v.id} value={v.id}>
                {v.name} ({v.category})
              </option>
            )) ?? <option>Loading...</option>}
          </Select>
        </div>

        <div className="flex flex-col gap-xs">
          <label className="font-semibold text-text">Speed ({speedInput.toFixed(1)}x)</label>
          <input
            type="range"
            min="0.5"
            max="2.0"
            step="0.1"
            value={speedInput}
            onChange={handleSpeedChange}
            className="w-full"
          />
        </div>
      </div>

      {/* ── Interaction Mode ───────────────────────────────────── */}
      <div className="flex flex-col gap-xs">
        <h3 className="font-semibold text-text">Interaction Mode</h3>

        <div className="flex flex-col gap-xs">
          <label className="font-semibold text-text">Mode</label>
          <Select
            value={voice?.mode ?? 'ptt'}
            onChange={(e) => handleModeChange(e.target.value)}
          >
            <option value="ptt">Push to Talk (Space bar)</option>
            <option value="vad">Voice Activity Detection (hands-free)</option>
          </Select>
        </div>

        {/* VAD model download — shown when VAD mode is selected */}
        {voice?.mode === 'vad' && (
          <div className="flex flex-col gap-xs">
            <label className="font-semibold text-text">Silero VAD Model</label>
            <p className="text-sm text-text-secondary">
              Neural-network voice detection for more accurate hands-free mode.
              Falls back to energy-based detection if not downloaded.
            </p>
            <div className="flex gap-2 items-center">
              <span className="flex-1 text-sm text-text-secondary">
                Silero VAD v5 (~2 MB)
              </span>
              <Button
                onClick={handleDownloadVad}
                disabled={downloading === 'vad' || vadDownloaded}
                variant="secondary"
                size="sm"
              >
                {downloading === 'vad' ? 'Downloading…' : vadDownloaded ? '✓ Downloaded' : 'Download'}
              </Button>
            </div>
          </div>
        )}

        <div className="flex flex-col gap-xs">
          <label className="font-semibold text-text flex items-center gap-2">
            <input
              type="checkbox"
              checked={voice?.autoSpeak ?? false}
              onChange={handleAutoSpeakChange}
            />
            Auto-speak responses
          </label>
          <p className="text-sm text-text-secondary">
            Automatically read LLM responses aloud using TTS.
          </p>
        </div>
      </div>

      {/* ── Audio Devices ──────────────────────────────────────── */}
      {(voice?.devices?.length ?? 0) > 0 && (
        <div className="flex flex-col gap-xs">
          <h3 className="font-semibold text-text">Audio Devices</h3>
          <div className="flex flex-col gap-xs">
            <label className="font-semibold text-text">Input Device</label>
            <Select value="" onChange={() => {}}>
              {voice?.devices?.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name} {d.isDefault ? '(default)' : ''}
                </option>
              ))}
            </Select>
          </div>
        </div>
      )}

      {/* ── Download Progress ──────────────────────────────────── */}
      {voice?.downloadProgress && (
        <div className="flex flex-col gap-xs">
          <p className="text-sm text-text-secondary">
            Downloading {voice.downloadProgress.modelId}…{' '}
            {voice.downloadProgress.percent.toFixed(0)}%
          </p>
          <div className="w-full h-1 bg-border rounded-sm overflow-hidden">
            <div
              className="h-full bg-accent rounded-sm transition-[width] duration-200 ease-in-out"
              style={{ width: `${voice?.downloadProgress?.percent ?? 0}%` }}
            />
          </div>
        </div>
      )}

      {/* ── Status ─────────────────────────────────────────────── */}
      <div className="flex flex-col gap-xs">
        <h3 className="font-semibold text-text">Status</h3>
        <div className="text-sm text-text-secondary">
          <div>Pipeline: {voice?.isActive ? '🟢 Active' : '⚪ Inactive'}</div>
          <div>STT Engine: {voice?.sttLoaded ? '✓ Loaded' : '✗ Not loaded'}</div>
          <div>TTS Engine: {voice?.ttsLoaded ? '✓ Loaded' : '✗ Not loaded'}</div>
          <div>Default STT: {defaultSttModel ?? '—'}</div>
          <div>Default Voice: {selectedVoice}</div>
          <div>State: {voice?.voiceState}</div>
        </div>
      </div>

      {/* ── Actions ────────────────────────────────────────────── */}
      <div className="flex gap-sm pt-md">
        <Button onClick={onClose} variant="secondary" size="sm">
          Close
        </Button>
      </div>
    </div>
  );
};
