export type Point = [number, number];

export type Shape =
  | { kind: "fill"; pts: Point[]; color: number }
  | { kind: "stroke"; pts: Point[]; closed: boolean; width_px: number; dash: [number, number] | null; color: number }
  | { kind: "text"; at: Point; text: string; size_px: number; color: number };

export type Scene = { shapes: Shape[] };

/** An annotation drawn over the play, in rink meters. */
export type Mark =
  | { kind: "zone"; at: Point; r_m: number; color: number }
  | { kind: "dot"; at: Point; label: string; color: number }
  | { kind: "arrow"; from: Point; to: Point; color: number };

export type View = { zoom: number; pan: { x: number; y: number }; flipped: boolean };

export const FIT: View = { zoom: 1, pan: { x: 0, y: 0 }, flipped: false };

/** The view options every wasm `scene`, `hit_test` and `to_rink` call takes. */
export type SceneOpts = {
  width: number;
  height: number;
  zoom: number;
  pan: Point;
  flipped: boolean;
  hidden: string[];
  marks: Mark[];
};

/** A 0xRRGGBBAA color as CSS. */
export function css(color: number): string {
  return `rgb(${color >>> 24} ${(color >>> 16) & 0xff} ${(color >>> 8) & 0xff} / ${(color & 0xff) / 255})`;
}

function trace(ctx: CanvasRenderingContext2D, pts: Point[], closed: boolean) {
  ctx.beginPath();
  for (const [x, y] of pts) ctx.lineTo(x, y);
  if (closed) ctx.closePath();
}

export function paint(ctx: CanvasRenderingContext2D, scene: Scene) {
  for (const shape of scene.shapes) {
    switch (shape.kind) {
      case "fill":
        trace(ctx, shape.pts, true);
        ctx.fillStyle = css(shape.color);
        ctx.fill();
        break;
      case "stroke":
        trace(ctx, shape.pts, shape.closed);
        ctx.lineWidth = shape.width_px;
        ctx.setLineDash(shape.dash ?? []);
        ctx.strokeStyle = css(shape.color);
        ctx.stroke();
        break;
      case "text":
        if (!shape.text) break;
        ctx.font = `${shape.size_px}px system-ui, -apple-system, "Segoe UI", sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillStyle = css(shape.color);
        ctx.fillText(shape.text, shape.at[0], shape.at[1]);
        break;
    }
  }
}
