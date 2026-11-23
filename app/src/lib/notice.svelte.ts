// Short notices for things others did for you (confirmed, sent you G, took something back)
// and for a growing streak. One at a time, gone after a few seconds (KONZEPT §7.5 sounds).
import { feedback } from "$lib/settings.svelte";
import type { SoundId } from "$lib/sound";

export type Tone = "xp" | "coin" | "streak" | "info";

export interface Notice {
  id: number;
  text: string;
  detail?: string;
  tone: Tone;
}

class Notices {
  current = $state<Notice | null>(null);
  private queue: Notice[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;
  private seq = 0;

  show(text: string, tone: Tone, detail?: string, sound?: SoundId) {
    this.queue.push({ id: ++this.seq, text, tone, detail });
    if (sound) feedback(sound);
    if (!this.current) this.next();
  }

  dismiss() {
    clearTimeout(this.timer);
    this.next();
  }

  private next() {
    this.current = this.queue.shift() ?? null;
    if (this.current) this.timer = setTimeout(() => this.next(), 4500);
  }
}

export const notices = new Notices();
