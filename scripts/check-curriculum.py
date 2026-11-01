#!/usr/bin/env python3
"""Structural checks for curriculum/: references between course, lessons, steps and plays."""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "curriculum"
KEBAB = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
STEP_TYPES = {"card", "watch", "choice", "spot", "contrast"}
FRONTMATTER_KEYS = {"id", "module", "title", "play", "wrong"}

problems = []


def problem(where, msg):
    problems.append(f"{where}: {msg}")


def load_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as e:
        problem(path.relative_to(ROOT.parent), f"unreadable JSON ({e})")
        return None


def parse_markdown(path):
    lines = path.read_text().splitlines()
    if not lines or lines[0] != "---" or "---" not in lines[1:]:
        problem(path.name, "missing frontmatter")
        return {}, {}
    end = lines.index("---", 1)
    meta = {}
    for line in lines[1:end]:
        key, sep, value = line.partition(":")
        if not sep:
            problem(path.name, f"frontmatter line is not key: value ({line!r})")
            continue
        meta[key.strip()] = value.strip().strip('"')
    sections, current = {}, None
    for line in lines[end + 1:]:
        if line.startswith("## "):
            current = line[3:].strip()
            sections[current] = []
        elif current is not None:
            sections[current].append(line)
    return meta, {k: "\n".join(v).strip() for k, v in sections.items()}


def check_steps(where, steps, play, sections, prefix, seen_ids):
    duration = play["duration_ms"]
    decision_times = {e["t_ms"] for e in play["events"] if e["kind"] == "decision_point"}
    choice_times = set()
    if not 7 <= len(steps) <= 11:
        problem(where, f"{len(steps)} steps, expected 7 to 11")
    for step in steps:
        sid = step.get("id", "")
        at = f"{where} {sid or '<no id>'}"
        if not KEBAB.match(sid) or not sid.startswith(prefix):
            problem(at, f"id must be kebab-case and start with {prefix!r}")
        if sid in seen_ids:
            problem(at, "duplicate step id")
        seen_ids.add(sid)
        kind = step.get("type")
        if kind not in STEP_TYPES:
            problem(at, f"unknown type {kind!r}")
            continue
        if "section" in step or kind in ("card", "contrast"):
            if step.get("section") not in sections:
                problem(at, f"section {step.get('section')!r} not in the Markdown")
            elif not sections[step["section"]]:
                problem(at, f"section {step['section']!r} is empty")
        if "at_ms" in step or kind in ("choice", "spot", "contrast"):
            t = step.get("at_ms")
            if not isinstance(t, int) or not 0 <= t <= duration:
                problem(at, f"at_ms {t!r} outside the play (0 to {duration})")
        if kind == "watch":
            a, b = step.get("from_ms"), step.get("to_ms")
            if not (isinstance(a, int) and isinstance(b, int) and 0 <= a < b <= duration):
                problem(at, f"watch {a!r} to {b!r} is not a segment within 0 to {duration}")
        if kind in ("choice", "spot") and not step.get("prompt"):
            problem(at, "empty prompt")
        if kind == "choice":
            choice_times.add(step.get("at_ms"))
            options = step.get("options", [])
            if not 3 <= len(options) <= 4:
                problem(at, f"{len(options)} options, expected 3 or 4")
            if sum(1 for o in options if o.get("grade") == 2) != 1:
                problem(at, "needs exactly one option with grade 2")
            for i, o in enumerate(options):
                if o.get("grade") not in (0, 1, 2):
                    problem(at, f"option {i} grade {o.get('grade')!r} is not 0, 1 or 2")
                if not o.get("text", "").strip() or not o.get("fb", "").strip():
                    problem(at, f"option {i} has an empty text or fb")
                if "play" in o and (o["play"] != "wrong" or o.get("grade") == 2):
                    problem(at, f"option {i} play must be \"wrong\" and only on a wrong or close option")
        if kind == "spot":
            for key in ("target", "r1_m", "r2_m"):
                if key in step:
                    problem(at, f"spot has {key}; the target is me's drafted position")
            fb = step.get("fb", {})
            for key in ("right", "close", "wrong"):
                if not fb.get(key, "").strip():
                    problem(at, f"spot fb.{key} is empty")
            if step.get("at_ms") not in decision_times:
                problem(at, f"spot at {step.get('at_ms')} is not a decision point {sorted(decision_times)}")
    for t in sorted(decision_times - choice_times):
        problem(where, f"decision point at {t} ms has no choice step")


def main():
    course = load_json(ROOT / "course.json")
    if course is None:
        return
    if course.get("schema") != 1:
        problem("course.json", "schema must be 1")
    module_of = {lid: m["id"] for m in course["modules"] for lid in m["lessons"]}
    lessons = {}
    for md in sorted((ROOT / "lessons").glob("*.md")):
        meta, sections = parse_markdown(md)
        missing = FRONTMATTER_KEYS - meta.keys()
        if missing:
            problem(md.name, f"frontmatter missing {sorted(missing)}")
            continue
        lessons[meta["id"]] = (md, meta, sections)
    for lid in module_of:
        if lid not in lessons:
            problem("course.json", f"lesson {lid!r} has no Markdown file")
    seen_ids = set()
    for lid, (md, meta, sections) in lessons.items():
        where = md.name
        if module_of.get(lid) != meta["module"]:
            problem(where, f"module {meta['module']!r} does not match course.json ({module_of.get(lid)!r})")
        if meta["play"] != md.stem or meta["wrong"] != f"{md.stem}.wrong":
            problem(where, "play and wrong must be the lesson's stem and stem.wrong")
        play = load_json(ROOT / "plays" / f"{meta['play']}.rink.json")
        wrong = load_json(ROOT / "plays" / f"{meta['wrong']}.rink.json")
        if play is None or wrong is None:
            continue
        me = [e["id"] for e in play["entities"] if e.get("is_me")]
        if me != [e["id"] for e in wrong["entities"] if e.get("is_me")]:
            problem(where, "play and wrong variant have a different me")
        if wrong["duration_ms"] != play["duration_ms"] or wrong["id"] == play["id"]:
            problem(where, "wrong variant must keep the duration and have its own id")
        for name in ("Principle", "Cue", "What wrong looks like", "Self-check"):
            if not sections.get(name):
                problem(where, f"missing or empty section {name!r}")
        steps_path = md.with_name(f"{md.stem}.steps.json")
        doc = load_json(steps_path)
        if doc is None:
            continue
        if doc.get("schema") != 1 or doc.get("lesson") != lid:
            problem(steps_path.name, f"schema must be 1 and lesson {lid!r}")
        prefix = lid.split("-", 1)[0] + "-"
        check_steps(steps_path.name, doc.get("steps", []), play, sections, prefix, seen_ids)
    for p in problems:
        print(p)
    print(f"{len(lessons)} lessons, {len(seen_ids)} steps, {len(problems)} problems")


main()
sys.exit(1 if problems else 0)
