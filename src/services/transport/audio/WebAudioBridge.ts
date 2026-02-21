/**
 * WebAudioBridge — browser-side audio I/O for the WebSocket voice data plane.
 *
 * Establishes a WebSocket binary connection to `/api/voice/audio`, captures
 * microphone audio via the Web Audio API, encodes it to PCM16 LE (16 kHz mono)
 * and streams 30 ms frames to the server.  Receives PCM16 LE (24 kHz mono)
 * playback frames from the server and renders them through a ring-buffer
 * AudioWorklet to prevent scheduling jitter.
 *
 * ## Wire protocol
 * - **Inbound  (client → server):** PCM16 LE, 16 kHz, mono, 960 bytes / frame
 *   (480 samples × 2 bytes, i.e. 30 ms).
 * - **Outbound (server → client):** PCM16 LE, 24 kHz, mono, variable length.
 *
 * ## AudioWorklet strategy
 * Playback frames are fed into a 2-second ring buffer owned by an
 * `AudioWorklet`.  `process()` drains the buffer at a constant rate set by the
 * browser's audio render quantum, outputting silence when starved rather than
 * introducing scheduling gaps.  An `overflow` message is posted to the main
 * thread when the buffer exceeds 80% capacity so it can be logged/monitored.
 *
 * ## Autoplay policy
 * Both `AudioContext` instances are resumed inside `connect()`, which *must*
 * be called from a user-gesture handler (button click, key press, etc.) to
 * satisfy the browser autoplay policy.
 *
 * ## Secure context
 * `getUserMedia` and `AudioWorklet` are only available in secure contexts
 * (HTTPS or localhost).  `connect()` guards this explicitly.
 *
 * @module services/transport/audio/WebAudioBridge
 */

// ── Wire protocol constants ────────────────────────────────────────────────────

/** Microphone sample rate streamed to the server (matches whisper input). */
const CAPTURE_SAMPLE_RATE = 16_000;

/** Playback sample rate received from the server (matches kokoro TTS output). */
const PLAYBACK_SAMPLE_RATE = 24_000;

/** Number of mono samples per outbound capture frame (30 ms at 16 kHz). */
const CAPTURE_FRAME_SAMPLES = 480;

// ── AudioWorklet inline source strings ────────────────────────────────────────
//
// NOTE: Loading AudioWorklets from inline Blob URLs requires that your
// Content-Security-Policy includes `worker-src blob:` (or `script-src blob:`
// on platforms that also gate workers on script-src).  If CSP violations appear
// in the browser console, add `worker-src blob:` to your server's CSP header.

/**
 * Capture worklet: accumulates `process()` input into 480-sample chunks and
 * posts each chunk as a transferable `ArrayBuffer` (Int16 LE) to the main
 * thread.
 */
const CAPTURE_WORKLET_SOURCE = `
class CaptureProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this._buf = new Float32Array(${CAPTURE_FRAME_SAMPLES});
    this._offset = 0;
  }

  process(inputs) {
    const ch = inputs[0]?.[0];
    if (!ch) return true;

    let src = 0;
    while (src < ch.length) {
      const take = Math.min(
        ch.length - src,
        ${CAPTURE_FRAME_SAMPLES} - this._offset,
      );
      this._buf.set(ch.subarray(src, src + take), this._offset);
      this._offset += take;
      src += take;

      if (this._offset >= ${CAPTURE_FRAME_SAMPLES}) {
        // Encode float32 → PCM16 LE (clamped)
        const pcm = new Int16Array(${CAPTURE_FRAME_SAMPLES});
        for (let i = 0; i < ${CAPTURE_FRAME_SAMPLES}; i++) {
          const s = Math.max(-1, Math.min(1, this._buf[i]));
          pcm[i] = s < 0 ? Math.round(s * 0x8000) : Math.round(s * 0x7fff);
        }
        this.port.postMessage(pcm.buffer, [pcm.buffer]);
        this._buf = new Float32Array(${CAPTURE_FRAME_SAMPLES});
        this._offset = 0;
      }
    }
    return true;
  }
}
registerProcessor('capture-processor', CaptureProcessor);
`;

/**
 * Playback worklet: owns a 2-second ring buffer (48 000 samples at 24 kHz).
 *
 * - Main thread pushes decoded `Float32Array` frames via `port.postMessage`.
 * - `process()` drains at the constant hardware render rate, writing silence
 *   when starved to prevent gaps.
 * - Posts `{ type: 'overflow' }` when the fill level exceeds 80% capacity so
 *   the main thread can log/monitor buffer pressure without disrupting playback.
 */
const PLAYBACK_RING_CAPACITY = PLAYBACK_SAMPLE_RATE * 2; // 2 s ring buffer

const PLAYBACK_WORKLET_SOURCE = `
const CAPACITY = ${PLAYBACK_RING_CAPACITY};
const OVERFLOW_THRESHOLD = Math.floor(CAPACITY * 0.8);

class PlaybackProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this._ring  = new Float32Array(CAPACITY);
    this._write = 0;
    this._read  = 0;
    this._fill  = 0;

    this.port.onmessage = (e) => {
      const frame = new Float32Array(e.data);

      if (this._fill + frame.length > CAPACITY) {
        // Ring buffer full — drop the incoming frame to avoid data corruption.
        this.port.postMessage({ type: 'overflow' });
        return;
      }

      for (let i = 0; i < frame.length; i++) {
        this._ring[this._write] = frame[i];
        this._write = (this._write + 1) % CAPACITY;
      }
      this._fill += frame.length;

      if (this._fill > OVERFLOW_THRESHOLD) {
        this.port.postMessage({ type: 'overflow' });
      }
    };
  }

  process(_inputs, outputs) {
    const out = outputs[0]?.[0];
    if (!out) return true;

    for (let i = 0; i < out.length; i++) {
      if (this._fill > 0) {
        out[i] = this._ring[this._read];
        this._ring[this._read] = 0;
        this._read = (this._read + 1) % CAPACITY;
        this._fill--;
      } else {
        out[i] = 0; // silence when starved
      }
    }
    return true;
  }
}
registerProcessor('playback-processor', PlaybackProcessor);
`;

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Wraps an AudioWorklet source string in a Blob URL for `addModule()`. */
function makeBlobUrl(source: string): string {
  const blob = new Blob([source], { type: 'application/javascript' });
  return URL.createObjectURL(blob);
}

/**
 * Derives the WebSocket endpoint URL dynamically from the current page origin,
 * switching between `ws:` and `wss:` to match the page protocol.
 */
function resolveWsUrl(): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${window.location.host}/api/voice/audio`;
}

// ── WebAudioBridge ────────────────────────────────────────────────────────────

/**
 * Browser-side audio bridge for the WebSocket voice data plane.
 *
 * Lifecycle:
 * 1. Call `connect()` from a user-gesture handler.
 * 2. Audio streaming begins automatically once the WS is open.
 * 3. Call `disconnect()` to stop all audio and close the connection.
 */
export class WebAudioBridge {
  private ws: WebSocket | null = null;
  private captureCtx: AudioContext | null = null;
  private playbackCtx: AudioContext | null = null;
  private captureWorklet: AudioWorkletNode | null = null;
  private playbackWorklet: AudioWorkletNode | null = null;
  private micStream: MediaStream | null = null;
  private captureBlobUrl: string | null = null;
  private playbackBlobUrl: string | null = null;

  // ── Static capability check ────────────────────────────────────────────────

  /**
   * Returns `true` when the browser environment supports the Web Audio API
   * features required by this bridge.
   *
   * Checks:
   * - Running in a **secure context** (HTTPS or localhost)
   * - `AudioWorkletNode` is available
   * - `navigator.mediaDevices.getUserMedia` is available (microphone access)
   *
   * This is intentionally a static check that does **not** request permissions;
   * it only tests API availability.
   */
  static isSupported(): boolean {
    if (typeof window === 'undefined') return false;
    if (!window.isSecureContext) return false;
    if (typeof AudioWorkletNode === 'undefined') return false;
    if (
      typeof navigator === 'undefined' ||
      typeof navigator.mediaDevices === 'undefined' ||
      typeof navigator.mediaDevices.getUserMedia !== 'function'
    ) return false;

    // Safari (non-Chromium) does not honour the `sampleRate` option passed to
    // `new AudioContext({ sampleRate })`.  It silently uses the hardware
    // native rate (44.1 kHz or 48 kHz), which causes both capture and
    // playback to operate at the wrong rate — STT quality is degraded and TTS
    // audio plays at the wrong speed/pitch.
    //
    // A software resampler is tracked in TODO(#230).  Until it lands, Safari
    // is explicitly unsupported so the UI can show a graceful "not supported"
    // state rather than letting the user start voice and hit an error mid-call.
    //
    // Detection: UA contains "Safari" but not "Chrome", "Chromium", "CriOS",
    // or "FxiOS" (all of which are Chromium-based or Firefox and do honour
    // the sampleRate constraint).
    const ua = navigator.userAgent;
    if (/Safari/i.test(ua) && !/Chrome|Chromium|CriOS|FxiOS/i.test(ua)) {
      return false;
    }

    return true;
  }

  // ── Public API ─────────────────────────────────────────────────────────────

  /**
   * Open the microphone, start both AudioContexts, load AudioWorklets, and
   * connect the WebSocket to `/api/voice/audio`.
   *
   * **Must be called from a user-gesture handler** (button click, key event,
   * etc.) so the browser's autoplay policy allows `AudioContext.resume()`.
   *
   * @throws If not in a secure context, if `getUserMedia` is denied, or if the
   *   WebSocket handshake fails.
   */
  async connect(): Promise<void> {
    if (this.isConnected()) return; // idempotent

    if (!window.isSecureContext) {
      throw new Error(
        'WebAudioBridge requires a secure context (HTTPS or localhost). ' +
          'Voice streaming over plaintext HTTP is not supported.',
      );
    }

    // 1. Capture AudioContext — 16 kHz for whisper STT input
    this.captureCtx = new AudioContext({ sampleRate: CAPTURE_SAMPLE_RATE });
    if (this.captureCtx.state === 'suspended') {
      await this.captureCtx.resume();
    }
    // Validate that the browser honoured the requested sample rate.
    // Some browsers (notably Safari on iOS/macOS) may ignore sampleRate and
    // use the hardware native rate (44.1 kHz or 48 kHz instead).
    // TODO(#230): implement a software resampler (e.g. a WebAssembly resampler
    //   AudioWorklet) so Safari and other non-compliant browsers can be
    //   re-enabled.  Until then isSupported() returns false on Safari.
    if (this.captureCtx.sampleRate !== CAPTURE_SAMPLE_RATE) {
      const actual = this.captureCtx.sampleRate;
      await this.captureCtx.close();
      throw new Error(
        `WebAudioBridge capture AudioContext sample rate mismatch: ` +
          `expected ${CAPTURE_SAMPLE_RATE} Hz but got ${actual} Hz. ` +
          `Your browser does not honor AudioContext({ sampleRate }); ` +
          `mic audio would be captured at the wrong rate and STT quality would be degraded.`,
      );
    }

    // 2. Playback AudioContext — 24 kHz for kokoro TTS output
    this.playbackCtx = new AudioContext({ sampleRate: PLAYBACK_SAMPLE_RATE });
    if (this.playbackCtx.state === 'suspended') {
      await this.playbackCtx.resume();
    }
    // Same validation for playback: a mismatched rate causes TTS audio to play
    // at the wrong speed/pitch.
    // TODO(#230): software resampler needed (same as capture above).
    if (this.playbackCtx.sampleRate !== PLAYBACK_SAMPLE_RATE) {
      const actual = this.playbackCtx.sampleRate;
      await this.playbackCtx.close();
      throw new Error(
        `WebAudioBridge playback AudioContext sample rate mismatch: ` +
          `expected ${PLAYBACK_SAMPLE_RATE} Hz but got ${actual} Hz. ` +
          `Your browser does not honor AudioContext({ sampleRate }), ` +
          `so 24 kHz PCM from the server would play at the wrong speed/pitch.`,
      );
    }

    // 3. Load AudioWorklet processors from inline Blob URLs
    this.captureBlobUrl  = makeBlobUrl(CAPTURE_WORKLET_SOURCE);
    this.playbackBlobUrl = makeBlobUrl(PLAYBACK_WORKLET_SOURCE);

    await this.captureCtx.audioWorklet.addModule(this.captureBlobUrl);
    await this.playbackCtx.audioWorklet.addModule(this.playbackBlobUrl);

    // 4. Microphone stream — disable browser processing for raw PCM fidelity
    this.micStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        sampleRate: CAPTURE_SAMPLE_RATE,
        channelCount: 1,
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
      },
    });

    // 5. Capture graph: mic → CaptureProcessor → (frames via port → WS)
    const micSource = this.captureCtx.createMediaStreamSource(this.micStream);
    this.captureWorklet = new AudioWorkletNode(
      this.captureCtx,
      'capture-processor',
    );
    this.captureWorklet.port.onmessage = (e: MessageEvent<ArrayBuffer>) => {
      if (this.isConnected()) {
        this.ws!.send(e.data);
      }
    };
    micSource.connect(this.captureWorklet);
    // CaptureProcessor has no audio output route — it only posts to the main
    // thread.  Connecting to destination is unnecessary but harmless; we omit
    // it to keep the graph clean.

    // 6. Playback graph: PlaybackProcessor → destination (speakers/headphones)
    this.playbackWorklet = new AudioWorkletNode(
      this.playbackCtx,
      'playback-processor',
      { numberOfOutputs: 1, outputChannelCount: [1] },
    );
    this.playbackWorklet.port.onmessage = (
      e: MessageEvent<{ type: string }>,
    ) => {
      if (e.data.type === 'overflow') {
        console.warn(
          '[WebAudioBridge] Playback ring buffer overflow — frame dropped.',
        );
      }
    };
    this.playbackWorklet.connect(this.playbackCtx.destination);

    // 7. WebSocket connection (binary frames only)
    this.ws = new WebSocket(resolveWsUrl());
    this.ws.binaryType = 'arraybuffer';

    await new Promise<void>((resolve, reject) => {
      this.ws!.onopen  = () => resolve();
      this.ws!.onerror = () =>
        reject(new Error('WebSocket connection to voice audio endpoint failed.'));
    });

    // 8. Route inbound PCM16 LE frames → playback ring buffer
    this.ws.onmessage = (e: MessageEvent<ArrayBuffer>) => {
      if (!this.playbackWorklet || !(e.data instanceof ArrayBuffer)) return;
      const pcm16 = new Int16Array(e.data);
      const f32   = new Float32Array(pcm16.length);
      for (let i = 0; i < pcm16.length; i++) {
        // Normalise to [-1, 1] matching the sign-aware divisor.
        f32[i] = pcm16[i] < 0
          ? pcm16[i] / 0x8000
          : pcm16[i] / 0x7fff;
      }
      this.playbackWorklet.port.postMessage(f32.buffer, [f32.buffer]);
    };

    // 9. Graceful server-side close: clean up audio without throwing.
    this.ws.onclose = () => {
      this._teardownAudio();
    };
  }

  /**
   * Gracefully close the WebSocket (code 1000) and release all audio
   * resources: mic tracks, AudioWorklet nodes, AudioContexts, Blob URLs.
   */
  disconnect(): void {
    if (this.ws) {
      // Suppress the onclose handler to avoid double teardown.
      this.ws.onclose = null;
      if (
        this.ws.readyState === WebSocket.OPEN ||
        this.ws.readyState === WebSocket.CONNECTING
      ) {
        this.ws.close(1000, 'client_disconnect');
      }
      this.ws = null;
    }
    this._teardownAudio();
  }

  /** Returns `true` when the WebSocket is in the `OPEN` state. */
  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  // ── Private helpers ────────────────────────────────────────────────────────

  /** Release all audio resources (called from both `disconnect` and `onclose`). */
  private _teardownAudio(): void {
    // Stop microphone tracks → OS mic indicator off
    if (this.micStream) {
      this.micStream.getTracks().forEach((t) => t.stop());
      this.micStream = null;
    }

    // Disconnect AudioWorklet nodes (best-effort; may already be disconnected)
    try { this.captureWorklet?.disconnect(); }  catch { /* ignore */ }
    try { this.playbackWorklet?.disconnect(); } catch { /* ignore */ }
    this.captureWorklet  = null;
    this.playbackWorklet = null;

    // Close AudioContexts asynchronously — errors are non-fatal
    this.captureCtx?.close().catch(() => { /* ignore */ });
    this.playbackCtx?.close().catch(() => { /* ignore */ });
    this.captureCtx  = null;
    this.playbackCtx = null;

    // Revoke Blob URLs to free memory
    if (this.captureBlobUrl)  { URL.revokeObjectURL(this.captureBlobUrl);  this.captureBlobUrl  = null; }
    if (this.playbackBlobUrl) { URL.revokeObjectURL(this.playbackBlobUrl); this.playbackBlobUrl = null; }
  }
}
