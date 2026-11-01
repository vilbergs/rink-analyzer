// Load the generated rink-wasm module in Node, draw one frame of the
// breakout example with its review, and grade the fixture lesson. Run
// scripts/build-wasm.sh first.
import { readFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root));

const { default: init, PlayHandle, LessonHandle } = await import(new URL("web/src/lib/wasm/rink_wasm.js", root));
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

const fixture = (name) => read(`crates/rink-lesson/tests/fixtures/${name}`).toString();
const lesson = LessonHandle.load(
  fixture("breakout.md"),
  fixture("breakout.steps.json"),
  fixture("breakout.rink.json"),
  fixture("breakout.wrong.rink.json"),
);
const spotStep = lesson.steps().find((s) => s.type === "spot");
console.log(`lesson: ${lesson.meta().title}, ${lesson.steps().length} steps, issues: ${JSON.stringify(lesson.check())}`);
console.log("right spot:", JSON.stringify(summary(lesson.grade(spotStep.id, { kind: "spot", at: spotStep.target }))));
console.log("wrong choice:", JSON.stringify(summary(lesson.grade("fx-choice-dp", { kind: "choice", index: 2 }))));
const { marks } = lesson.grade(spotStep.id, { kind: "spot", at: [-10, 0] });
const marked = lesson.scene("main", spotStep.at_ms, { ...view, hidden: ["home_LW"], marks });
console.log("spot answer scene, last shape:", marked.shapes.at(-1).kind, marked.shapes.at(-1).text ?? "");

function summary({ grade, feedback, watch, marks, facts }) {
  return { grade, feedback, watch, marks: marks.map((m) => m.kind), distance_m: facts?.distance_m };
}

function firstTextAt(scene, text) {
  const shape = scene.shapes.find((s) => s.kind === "text" && s.text === text);
  if (!shape) throw new Error(`no ${text} label in the scene`);
  return shape.at;
}
