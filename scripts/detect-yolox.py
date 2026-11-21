#!/usr/bin/env python3
"""Emit person detections for upright frames using the official YOLOX-S ONNX export."""

import argparse
import json
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image


def anchors(height, width):
    cells = []
    strides = []
    for stride in (8, 16, 32):
        rows, columns = np.indices((height // stride, width // stride))
        cells.append(np.stack((columns.ravel(), rows.ravel()), axis=1))
        strides.append(np.full((rows.size, 1), stride))
    return np.concatenate(cells), np.concatenate(strides)


def person_boxes(predictions, cells, strides, ratio, image_size, confidence):
    # YOLOX-S export uses raw grid-relative centers and log box sizes.
    scores = predictions[:, 4] * predictions[:, 5]
    keep = (scores >= confidence) & (predictions[:, 5:].argmax(axis=1) == 0)
    centers = (predictions[keep, :2] + cells[keep]) * strides[keep] / ratio
    half_sizes = np.exp(predictions[keep, 2:4]) * strides[keep] / (2 * ratio)
    lows = np.clip(centers - half_sizes, 0, image_size)
    highs = np.clip(centers + half_sizes, 0, image_size)
    areas = np.prod(highs - lows, axis=1)
    scores = scores[keep]
    order = np.flatnonzero(areas > 0)
    order = order[np.argsort(scores[order])[::-1]]
    result = []
    while order.size:
        best, rest = order[0], order[1:]
        box = [*lows[best], *highs[best]]
        result.append({"xyxy": [round(float(v), 2) for v in box], "confidence": round(float(scores[best]), 4)})
        overlap = np.maximum(0, np.minimum(highs[best], highs[rest]) - np.maximum(lows[best], lows[rest]))
        intersection = np.prod(overlap, axis=1)
        order = rest[intersection / (areas[best] + areas[rest] - intersection) <= 0.45]
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", type=Path)
    parser.add_argument("frames", type=Path, nargs="+")
    parser.add_argument("--confidence", type=float, default=0.25)
    args = parser.parse_args()
    if not 0 < args.confidence <= 1:
        parser.error("--confidence must be greater than 0 and at most 1")

    ort.preload_dlls(directory="")
    if "CUDAExecutionProvider" not in ort.get_available_providers():
        raise SystemExit("ONNX Runtime has no CUDA execution provider")
    session = ort.InferenceSession(str(args.model), providers=["CUDAExecutionProvider"])
    if session.get_providers()[0] != "CUDAExecutionProvider":
        raise SystemExit("CUDA could not be initialized")
    session.disable_fallback()
    model_input = session.get_inputs()[0]
    _, channels, height, width = model_input.shape
    if channels != 3 or not isinstance(height, int) or not isinstance(width, int):
        raise SystemExit("Expected a fixed-size YOLOX-S image input")
    cells, strides = anchors(height, width)

    for frame in (item for arg in args.frames for item in (sorted(arg.glob("*.jpg"), key=lambda frame: int(frame.stem.rsplit("-", 1)[-1])) if arg.is_dir() else [arg])):
        with Image.open(frame) as image:
            image_size = image.size
            ratio = min(width / image.width, height / image.height)
            size = (max(1, int(image.width * ratio)), max(1, int(image.height * ratio)))
            resized = np.asarray(image.convert("RGB").resize(size, Image.Resampling.BILINEAR))
        canvas = np.full((height, width, 3), 114, dtype=np.float32)
        canvas[:size[1], :size[0]] = resized[..., ::-1]  # BGR, top-left letterbox
        tensor = np.ascontiguousarray(canvas.transpose(2, 0, 1)[None, ...])
        output, = session.run(None, {model_input.name: tensor})
        if output.shape != (1, len(cells), 85):
            raise SystemExit(f"Unexpected YOLOX-S output shape {output.shape}")
        boxes = person_boxes(output[0], cells, strides, ratio, image_size, args.confidence)
        print(json.dumps({"frame": frame.name, "provider": session.get_providers()[0], "detections": boxes}), flush=True)


if __name__ == "__main__":
    main()
