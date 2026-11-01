/** A Markdown section's text as paragraphs, split on blank lines. */
export function paragraphs(text: string): string[] {
  return text.split(/\n\s*\n/).filter((p) => p.trim() !== "");
}
