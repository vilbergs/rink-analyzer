import type { Mark, Point } from "../rink/paint";

// The JSON shapes `LessonHandle` returns, as rink-lesson serializes them.

export type Which = "main" | "wrong";

/** 0 wrong, 1 close, 2 right. */
export type Grade = 0 | 1 | 2;

export type LessonMeta = { id: string; module: string; title: string; sections: Record<string, string> };

export type ChoiceOption = { text: string; grade: Grade; fb: string; play: Which };

export type Step =
  | { type: "card"; id: string; section: string }
  | { type: "watch"; id: string; from_ms: number; to_ms: number; play: Which; speed: number }
  | { type: "choice"; id: string; at_ms: number; prompt: string; options: ChoiceOption[] }
  | {
      type: "spot";
      id: string;
      at_ms: number;
      prompt: string;
      target: Point | null;
      r1_m: number;
      r2_m: number;
      fb: { right: string; close: string; wrong: string };
    }
  | { type: "contrast"; id: string; at_ms: number; section: string; to_ms: number };

export type GradedStep = Extract<Step, { type: "choice" | "spot" }>;

export type Answer = { kind: "choice"; index: number } | { kind: "spot"; at: Point };

export type EntityFacts = {
  id: string;
  is_me?: boolean;
  pos: Point;
  side: "strong" | "weak" | "middle";
  relative_to_puck: "above" | "level" | "below";
  dist_to_puck_m: number;
  nearest_opponent?: { id: string; dist_m: number };
  open_pass_lane_from_carrier?: boolean;
};

export type Graded = {
  grade: Grade;
  feedback: string;
  watch: { play: Which; from_ms: number } | null;
  marks: Mark[];
  facts: { actual: EntityFacts; target: EntityFacts; distance_m: number } | null;
};

export type Caption = { t_ms: number; text: string };

/** An entity as the `.rink.json` file stores it. */
export type PlayEntity = { id: string; team?: "home" | "away"; role?: string; number?: number; is_me?: boolean };
