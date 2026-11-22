// Fairteilt sounds (KONZEPT §7.5): a tiny Kalimba synth on Web Audio, no audio files.
// Recipes are the ones chosen in the sound playground (Kalimba · Mittel · Fröhlich).
// Overtake and family goal share one motif (after Kenney "Music Jingles – Pizzicato 02", CC0).

export type SoundId = "step" | "done" | "confirm" | "coin" | "streak" | "overtake" | "goal";

type Note = [name: string, offset: number, velocity: number, decay: number];

const MASTER = 0.24;
const SHIFT = -5; // "Mittel": a fourth below the written notes

const SONGS: Record<SoundId, Note[]> = {
  step: [["E5", 0, 0.45, 0.35], ["G5", 0.06, 0.6, 0.55]],
  done: [["G5", 0, 0.8, 0.2], ["C6", 0.07, 1, 0.34], ["E6", 0.07, 0.4, 0.3]],
  confirm: [["G4", 0, 0.75, 0.5], ["C5", 0.1, 0.85, 0.6], ["E5", 0.2, 0.9, 1.0]],
  coin: [["B5", 0, 0.6, 0.3], ["E6", 0.05, 0.75, 0.4], ["B6", 0.11, 0.55, 0.9]],
  streak: [["C5", 0, 0.7, 0.5], ["E5", 0.09, 0.75, 0.5], ["G5", 0.2, 0.8, 0.5], ["A5", 0.29, 0.8, 0.5], ["C6", 0.4, 0.95, 1.1]],
  // Half-way: the run lands openly on G major ("weiter geht's").
  overtake: [
    ["C5", 0, 0.7, 0.2], ["D5", 0.07, 0.8, 0.2], ["D#5", 0.13, 0.8, 0.2], ["E5", 0.19, 0.8, 0.2], ["F#5", 0.25, 0.85, 0.2],
    ["G5", 0.32, 0.9, 0.8], ["G4", 0.32, 0.4, 0.7], ["B4", 0.34, 0.42, 0.7], ["D5", 0.36, 0.45, 0.7], ["D6", 0.37, 0.3, 0.6],
  ],
  // Full way: the same run arrives via A#–B on C and opens into C major. Nothing after the chord.
  goal: [
    ["C5", 0, 0.7, 0.2], ["D5", 0.08, 0.8, 0.2], ["D#5", 0.15, 0.8, 0.2], ["E5", 0.21, 0.8, 0.2], ["F#5", 0.27, 0.8, 0.2],
    ["G#5", 0.35, 0.8, 0.2], ["A#5", 0.43, 0.8, 0.2], ["B5", 0.5, 0.85, 0.2],
    ["C6", 0.57, 0.9, 1.1], ["C4", 0.57, 0.4, 1.0], ["G4", 0.59, 0.42, 1.0], ["C5", 0.61, 0.45, 1.0], ["E5", 0.63, 0.45, 1.0],
    ["G5", 0.65, 0.45, 1.0], ["E6", 0.65, 0.35, 0.9],
  ],
};

let ctx: AudioContext | null = null;
let bus: GainNode | null = null;

function audio(): AudioContext | null {
  if (typeof window === "undefined") return null;
  if (!ctx) {
    const C = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!C) return null;
    ctx = new C();
    const lowpass = ctx.createBiquadFilter();
    lowpass.type = "lowpass";
    lowpass.frequency.value = 3200;
    lowpass.Q.value = 0.5;
    const comp = ctx.createDynamicsCompressor();
    comp.threshold.value = -20;
    comp.knee.value = 12;
    comp.ratio.value = 4;
    comp.attack.value = 0.001;
    comp.release.value = 0.25;
    const makeup = ctx.createGain();
    makeup.gain.value = 1.35;
    bus = ctx.createGain();
    bus.gain.value = MASTER;
    bus.connect(lowpass).connect(comp).connect(makeup).connect(ctx.destination);
  }
  if (ctx.state === "suspended") void ctx.resume();
  return ctx;
}

const NOTE_INDEX: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };

/** Frequency of a note name like "C5" or "F#4", shifted by `shift` semitones. */
export function noteFrequency(note: string, shift = 0): number {
  const m = /^([A-G])(#?)(\d)$/.exec(note);
  if (!m) throw new Error(`bad note ${note}`);
  const midi = NOTE_INDEX[m[1]] + (m[2] ? 1 : 0) + (Number(m[3]) + 1) * 12 + shift;
  return 440 * Math.pow(2, (midi - 69) / 12);
}

/** Kalimba: fundamental, a soft octave and a short metallic "tine" click. */
function kalimba(c: AudioContext, f: number, t: number, vel: number, dur: number) {
  const out = c.createGain();
  out.gain.value = vel;
  out.connect(bus!);
  for (const [mult, amp, len] of [[1, 1, dur * 1.3], [2, 0.1, dur * 0.5], [5.95, 0.22, 0.09]] as const) {
    const o = c.createOscillator();
    const g = c.createGain();
    o.type = "sine";
    o.frequency.value = f * mult;
    g.gain.setValueAtTime(0.0001, t);
    g.gain.exponentialRampToValueAtTime(amp, t + 0.003);
    g.gain.exponentialRampToValueAtTime(0.0001, t + len);
    o.connect(g).connect(out);
    o.start(t);
    o.stop(t + len + 0.05);
  }
}

/** Plays a sound. Returns its length in seconds (0 when audio is unavailable). */
export function playSound(id: SoundId): number {
  let c: AudioContext | null = null;
  try {
    c = audio();
  } catch {
    c = null;
  }
  if (!c) return 0;
  const t0 = c.currentTime + 0.03;
  let end = 0;
  for (const [note, offset, vel, decay] of SONGS[id]) {
    kalimba(c, noteFrequency(note, SHIFT), t0 + offset, vel, decay);
    end = Math.max(end, offset + decay);
  }
  return end;
}
