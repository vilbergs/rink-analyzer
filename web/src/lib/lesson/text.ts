import type { Step } from "./types";

/** A Markdown section's text as paragraphs, split on blank lines. */
export function paragraphs(text: string): string[] {
  return text.split(/\n\s*\n/).filter((p) => p.trim() !== "");
}

/** `text` split after its first sentence: the lead, then the rest. */
export function firstSentence(text: string): [string, string] {
  const lead = /^.*?[.!?](?=\s|$)/s.exec(text.trim())?.[0] ?? text.trim();
  return [lead, text.trim().slice(lead.length).trim()];
}

export const seconds = (ms: number) => `${(ms / 1000).toFixed(1)} s`;

/** What a step is called where the user sees its kind: the expanded rink's button, the result's rows. */
export function stepName(step: Step): string {
  switch (step.type) {
    case "card":
    case "contrast":
      return step.section;
    case "watch":
      return `Watch · ${(step.from_ms / 1000).toFixed(1)} to ${seconds(step.to_ms)}`;
    case "choice":
      return "Choice";
    case "spot":
      return "Spot";
  }
}
