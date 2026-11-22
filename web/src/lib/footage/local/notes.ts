export type Moment = { id: string; ms: number; note: string };

export function timestamp(ms: number): string {
  const tenths = Math.floor(ms / 100);
  const seconds = Math.floor(tenths / 10);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const mm = hours ? String(minutes % 60).padStart(2, "0") : String(minutes);
  return `${hours ? hours + ":" : ""}${mm}:${String(seconds % 60).padStart(2, "0")}.${tenths % 10}`;
}

export function fileKey(file: File): string {
  return "rink.footage.v1." + JSON.stringify([file.name, file.size, file.lastModified]);
}

// A separate key per moment lets two open reviews add notes independently.
export function readMoments(prefix: string): Moment[] {
  const moments: Moment[] = [];
  for (const key of Object.keys(localStorage)) {
    if (!key.startsWith(prefix)) continue;
    try {
      const item = JSON.parse(localStorage.getItem(key) ?? "null");
      if (Number.isSafeInteger(item?.ms) && item.ms >= 0 && typeof item.note === "string") {
        moments.push({ id: key.slice(prefix.length), ms: item.ms, note: item.note });
      }
    } catch (error) {
      if (!(error instanceof SyntaxError)) throw error;
    }
  }
  return moments.sort((a, b) => a.ms - b.ms);
}

export function exportNotes(file: File, moments: Moment[]) {
  const text = [file.name, "", ...moments.map((m) => timestamp(m.ms) + "\t" + m.note), ""].join("\n");
  const url = URL.createObjectURL(new Blob([text], { type: "text/plain;charset=utf-8" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = (file.name.replace(/\.[^.]+$/, "") || file.name) + "-notes.txt";
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
