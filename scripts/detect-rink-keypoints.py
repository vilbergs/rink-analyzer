#!/usr/bin/env python3
"""Run the SimulaMet HockeyRink ONNX checkpoint and emit indexed image landmarks."""

import argparse
import json
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", type=Path)
    parser.add_argument("frames", type=Path, nargs="+")
    args = parser.parse_args()
    ort.preload_dlls(directory="")
    runtime = ort.InferenceSession(str(args.model), providers=["CUDAExecutionProvider"])
    if runtime.get_providers()[0] != "CUDAExecutionProvider":
        raise SystemExit("CUDA could not be initialized")
    runtime.disable_fallback()
    tensor_info = runtime.get_inputs()[0]
    _, channels, height, width = tensor_info.shape
    if channels != 3 or not isinstance(height, int) or not isinstance(width, int):
        raise SystemExit("Expected fixed-size HockeyRink input")

    for path in args.frames:
        with Image.open(path) as frame:
            original_w, original_h = frame.size
            ratio = min(width / original_w, height / original_h)
            w, h = round(original_w * ratio), round(original_h * ratio)
            frame = np.asarray(frame.convert("RGB").resize((w, h), Image.Resampling.BILINEAR))
        left, top = (width - w) // 2, (height - h) // 2
        canvas = np.full((height, width, 3), 114, dtype=np.uint8)
        canvas[top:top + h, left:left + w] = frame
        tensor = np.ascontiguousarray(canvas.astype(np.float32).transpose(2, 0, 1)[None, ...] / 255)
        output, = runtime.run(None, {tensor_info.name: tensor})
        if output.shape[1] != 173:
            raise SystemExit(f"Expected 56 HockeyRink keypoints, got {output.shape}")
        candidates = output[0].T
        best = candidates[np.argmax(candidates[:, 4])]
        locations = []
        if best[4] >= 0.30:
            for index, (x, y, score) in enumerate(best[5:].reshape(56, 3)):
                xy = ((x - left) / ratio, (y - top) / ratio)
                if score >= 0.40 and 0 <= xy[0] < original_w and 0 <= xy[1] < original_h:
                    locations.append({"id": index, "xy": [round(float(v), 2) for v in xy],
                                      "confidence": round(float(score), 4)})
        print(json.dumps({"frame": path.name, "rink_confidence": float(best[4]), "landmarks": locations}), flush=True)


if __name__ == "__main__":
    main()
