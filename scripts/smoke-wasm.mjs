// Load the generated rink-wasm module in Node and draw one frame of the
// breakout example with its review. Run scripts/build-wasm.sh first.
import { readFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root));

const { default: init, PlayHandle } = await import(new URL("web/src/lib/wasm/rink_wasm.js", root));
await init({ module_or_path: read("web/src/lib/wasm/rink_wasm_bg.wasm") });

const play = PlayHandle.from_json(read("examples/breakout.rink.json").toString());
const review = read("examples/breakout.review.json").toString();
const view = { width: 1200, height: 600 };
const scene = play.scene(3000, { ...view, selected: "home_LW", review });

console.log(`title: ${play.title()}, duration: ${play.duration_ms()} ms`);
console.log(`shapes: ${scene.shapes.length}`);
console.log("first text:", JSON.stringify(scene.shapes.find((s) => s.kind === "text")));
console.log("hit at home_LW:", play.hit_test(3000, ...firstTextAt(scene, "LW"), view));
console.log("center ice in meters:", play.to_rink(600, 300, view));

function firstTextAt(scene, text) {
  const shape = scene.shapes.find((s) => s.kind === "text" && s.text === text);
  if (!shape) throw new Error(`no ${text} label in the scene`);
  return shape.at;
}
