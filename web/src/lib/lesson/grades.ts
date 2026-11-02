import type { Grade } from "./types";

export const GRADES: Record<Grade, { label: string; color: string }> = {
  2: { label: "Right", color: "var(--feedback)" },
  1: { label: "Close", color: "var(--warn)" },
  0: { label: "Wrong", color: "var(--wrong)" },
};
