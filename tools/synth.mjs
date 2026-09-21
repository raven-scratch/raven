// Turn a list of notes into a WAV file.
//
//   import { render, write } from "../../../tools/synth.mjs";
//
// Scratch plays a sound by decoding the file it was given, so a chiptune has to
// be *audio* — samples — rather than a note list the runtime understands. This is
// the small synthesizer that makes those samples: a few waveforms, a list of
// notes per voice, an envelope so a note does not click at either end, and a WAV
// header around the result.
//
// It is deliberately the least a chiptune needs:
//
//   * `sine` and `triangle` are drawn from the phase directly and cannot alias.
//   * `square` and `saw` are built from their harmonics up to Nyquist, because a
//     naive one is a step function sampled at the rate — it folds every harmonic
//     above half the sample rate back down as a whistle that is not in the tune.
//   * A voice is monophonic, and several voices are summed, which is how a melody
//     and a bass line are two entries rather than one clever one.
//
// Everything is mono 16-bit PCM, which is what a browser decodes without help.

/** The sample rate the tunes are written at. A chiptune has nothing above 5 kHz. */
export const TUNE_RATE = 11025;

/** The sample rate the short effects are written at: they are tiny either way. */
export const EFFECT_RATE = 22050;

/** Middle C, for a reader who wants to write a tune. */
export const C4 = 60;

/** Frequency of a MIDI note, in Hz. */
export function frequency(midi) {
  return 440 * 2 ** ((midi - 69) / 12);
}

/** One sample of a waveform, given its phase in cycles. */
function wave(kind, phase, hz, nyquist) {
  if (kind === "sine") return Math.sin(2 * Math.PI * phase);
  if (kind === "triangle") return 4 * Math.abs((phase % 1) - 0.5) - 1;
  if (kind === "saw") {
    // Sum the harmonics that fit, so nothing folds back down.
    const limit = Math.min(24, Math.floor(nyquist / hz));
    let sum = 0;
    for (let k = 1; k <= limit; k += 1) sum += Math.sin(2 * Math.PI * k * phase) / k;
    return (sum * 2) / Math.PI;
  }
  const limit = Math.min(24, Math.floor(nyquist / hz));
  let sum = 0;
  for (let k = 1; k <= limit; k += 2) sum += Math.sin(2 * Math.PI * k * phase) / k;
  return (sum * 4) / Math.PI;
}

/** How loud a note is `n` samples in, out of `total`: a short fade at each end. */
function envelope(n, total, rate) {
  const attack = Math.min(Math.floor(rate * 0.004), Math.floor(total / 4));
  const release = Math.min(Math.floor(rate * 0.02), Math.floor(total / 4));
  if (n < attack) return n / attack;
  if (n > total - release) return (total - n) / release;
  return 1;
}

/**
 * Render voices into 16-bit samples.
 *
 * `voices` is a list of `{ wave, level, notes }`, where a note is `[midi, beats]`
 * and a midi of 0 is a rest. All the voices start together and are summed, so
 * the bass of a tune is its own entry rather than an offset in the melody.
 */
export function render({ tempo, rate = TUNE_RATE, voices, level = 0.85 }) {
  const secondsPerBeat = 60 / tempo;
  const frames = Math.max(
    ...voices.map((voice) =>
      Math.round(
        voice.notes.reduce((total, [, beats]) => total + beats, 0) * secondsPerBeat * rate,
      ),
    ),
  );
  const mix = new Float64Array(frames);

  for (const voice of voices) {
    const gain = voice.level ?? 1;
    let at = 0;
    for (const [midi, beats] of voice.notes) {
      const length = Math.round(beats * secondsPerBeat * rate);
      if (midi > 0 && length > 0) {
        const hz = frequency(midi);
        const nyquist = rate / 2;
        for (let n = 0; n < length && at + n < frames; n += 1) {
          const sample = wave(voice.wave ?? "square", (hz * n) / rate, hz, nyquist);
          mix[at + n] += sample * gain * envelope(n, length, rate);
        }
      }
      at += length;
    }
  }

  // One scale for the whole mix, so two voices keep the balance they were given.
  let peak = 0;
  for (const sample of mix) peak = Math.max(peak, Math.abs(sample));
  const scale = peak > 0 ? level / peak : 0;

  const data = Buffer.alloc(frames * 2);
  for (let n = 0; n < frames; n += 1) {
    const value = Math.max(-1, Math.min(1, mix[n] * scale));
    data.writeInt16LE(Math.round(value * 32767), n * 2);
  }
  return data;
}

/** A WAV file around 16-bit mono samples. */
export function wav(samples, rate) {
  const header = Buffer.alloc(44);
  header.write("RIFF", 0, "ascii");
  header.writeUInt32LE(36 + samples.length, 4);
  header.write("WAVE", 8, "ascii");
  header.write("fmt ", 12, "ascii");
  header.writeUInt32LE(16, 16); // the size of a PCM format block
  header.writeUInt16LE(1, 20); // 1 is uncompressed PCM
  header.writeUInt16LE(1, 22); // one channel
  header.writeUInt32LE(rate, 24);
  header.writeUInt32LE(rate * 2, 28); // bytes a second
  header.writeUInt16LE(2, 32); // bytes a frame
  header.writeUInt16LE(16, 34); // bits a sample
  header.write("data", 36, "ascii");
  header.writeUInt32LE(samples.length, 40);
  return Buffer.concat([header, samples]);
}

/**
 * What `wav` wrote, read back: the format, and the samples as −1 to 1.
 *
 * This is the other half of the writer, and it exists so a test can ask a file
 * what it actually contains rather than trusting that it was written.
 */
export function read(buffer) {
  if (buffer.toString("ascii", 0, 4) !== "RIFF" || buffer.toString("ascii", 8, 12) !== "WAVE") {
    throw new Error("not a RIFF/WAVE file");
  }
  const rate = buffer.readUInt32LE(24);
  const channels = buffer.readUInt16LE(22);
  const bits = buffer.readUInt16LE(34);
  const bytes = buffer.readUInt32LE(40);
  const frames = bytes / 2;
  const samples = new Float64Array(frames);
  for (let n = 0; n < frames; n += 1) samples[n] = buffer.readInt16LE(44 + n * 2) / 32768;
  return { rate, channels, bits, frames, seconds: frames / rate, samples };
}

/** How much of `hz` is in a stretch of samples: the Goertzel transform. */
export function tone(samples, rate, hz, start, length) {
  const w = (2 * Math.PI * hz) / rate;
  const c = 2 * Math.cos(w);
  let s1 = 0;
  let s2 = 0;
  for (let n = 0; n < length; n += 1) {
    const s0 = (samples[start + n] || 0) + c * s1 - s2;
    s2 = s1;
    s1 = s0;
  }
  return Math.sqrt(Math.max(0, s1 * s1 + s2 * s2 - c * s1 * s2));
}
