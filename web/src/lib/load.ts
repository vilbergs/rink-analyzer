import init from "./wasm/rink_wasm.js";

let ready: Promise<unknown> | null = null;

/** Instantiate the wasm module once, however many screens ask for it. */
export function wasmReady(): Promise<unknown> {
  ready ??= init();
  return ready;
}

/** A file shipped with the app, relative to its base URL. */
export async function fetchText(path: string): Promise<string> {
  const res = await fetch(import.meta.env.BASE_URL + path);
  if (!res.ok) throw new Error(`${path}: ${res.status} ${res.statusText}`);
  return res.text();
}
