/** Per-viewer preferences, kept in this browser. Progress lives in the ProgressStore, not here. */

export type Shot = "left" | "right";

const SHOT_KEY = "rink.shot";

function readShot(): Shot | null {
  try {
    const v = localStorage.getItem(SHOT_KEY);
    return v === "left" || v === "right" ? v : null;
  } catch {
    return null;
  }
}

class Settings {
  /** Which way the user shoots, or null until they say. A lesson's Shot card shows that hand's paragraph. */
  shot = $state<Shot | null>(readShot());

  setShot(shot: Shot | null) {
    this.shot = shot;
    try {
      if (shot) localStorage.setItem(SHOT_KEY, shot);
      else localStorage.removeItem(SHOT_KEY);
    } catch {
      // Storage can be blocked or full; the setting then lasts for this visit only.
    }
  }
}

export const settings = new Settings();
