/**
 * Audio transport factory.
 *
 * Returns a `WebAudioBridge` on browser (web UI) and `null` on Tauri desktop,
 * where the native cpal/rodio audio stack is used instead via the local
 * `AudioSource` / `AudioSink` implementations.
 *
 * **This module is the sole permitted location for platform branching on audio
 * I/O.**  All callers should import `createAudioBridge` from here rather than
 * constructing `WebAudioBridge` directly, so that the native path is
 * automatically skipped without scattered `isTauri()` guards.
 *
 * @module services/transport/audio
 */

import { WebAudioBridge } from './WebAudioBridge';

/** Returns `true` when running inside a Tauri desktop application. */
function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Create the audio bridge appropriate for the current platform.
 *
 * @returns A new `WebAudioBridge` instance on browser, or `null` on Tauri
 *   desktop (where the Rust audio stack handles I/O directly).
 */
export function createAudioBridge(): WebAudioBridge | null {
  if (isTauri()) return null;
  return new WebAudioBridge();
}

/**
 * Returns `true` when the current platform supports the audio I/O required
 * for voice mode.
 *
 * - **Tauri desktop**: always `true` — the native cpal/rodio stack is always
 *   available.
 * - **Browser**: delegates to {@link WebAudioBridge.isSupported}, which checks
 *   for a secure context, `AudioWorkletNode`, `getUserMedia`, and the absence
 *   of a Safari UA.  Safari (non-Chromium) does not honour the `sampleRate`
 *   option passed to `new AudioContext({ sampleRate })`, which would cause STT
 *   capture and TTS playback to operate at the wrong sample rate.  Safari
 *   support requires a software resampler (tracked in TODO #230) and is
 *   currently disabled so callers receive `false` and can show a graceful
 *   "not supported" UI rather than an in-call error.
 */
export function isAudioSupported(): boolean {
  if (isTauri()) return true; // native audio stack — always available
  return WebAudioBridge.isSupported();
}

export type { WebAudioBridge };
