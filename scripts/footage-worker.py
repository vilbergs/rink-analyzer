#!/usr/bin/env python3
"""Pull one tracking job at a time, run Furulid's detector, and publish its preview."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
from threading import Event, Thread
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from urllib.parse import quote

ROOT = Path(__file__).resolve().parent
API = os.environ["RINK_FOOTAGE_API"].rstrip("/") + "/api/footage/worker"
TOKEN = os.environ["RINK_FOOTAGE_WORKER_TOKEN"]
FPS = 15


def request(route, method="GET", lease=None, body=None, mime="application/octet-stream", title=None, failure=None):
    headers = {"Authorization": f"Bearer {TOKEN}"}
    if lease:
        headers["X-Rink-Lease"] = lease
    if body:
        headers["Content-Length"] = str(os.fstat(body.fileno()).st_size)
        headers["Content-Type"] = mime
    if failure:
        headers["X-Rink-Import-Failure"] = failure
    if title:
        headers["X-Rink-Title"] = quote(title[:100], safe="")
    return urlopen(Request(API + route, data=body, headers=headers, method=method), timeout=90)


def heartbeat(route, lease, stop):
    while not stop.wait(40):
        try:
            with request(route + "/heartbeat", "POST", lease):
                pass
        except OSError as error:
            print(f"Could not renew the tracking lease: {error}", file=sys.stderr, flush=True)


def extract_frames(source, frames):
    try:
        subprocess.run([ROOT / "rink-footage", source, frames, "720", "--fps", str(FPS)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, check=True, timeout=2700)
        return
    except subprocess.CalledProcessError:
        if frames.exists():
            shutil.rmtree(frames)
        frames.mkdir()
    # iOS and other editors may hand us H.264 or fragmented MOV/MP4.
    filters = f"fps={FPS},scale=720:720:force_original_aspect_ratio=decrease:force_divisible_by=2,setsar=1"
    subprocess.run([
        "ffmpeg", "-nostdin", "-v", "error", "-f", "mov", "-protocol_whitelist", "file,pipe",
        "-enable_drefs", "0", "-threads", "2", "-i", source, "-map", "0:v:0", "-an", "-sn",
        "-filter_threads", "2", "-vf", filters, "-q:v", "3", "-start_number", "0", frames / "%08d.jpg",
    ], check=True, timeout=2700)
    for index, frame in enumerate(sorted(frames.glob("*.jpg"))):
        time_ms = (index * 1000 + FPS // 2) // FPS
        frame.rename(frames / f"{source.stem}-{time_ms:06d}.jpg")


def detect_camera_cuts(frames, output):
    frame_files = sorted(frames.glob("source-*.jpg"))
    result = subprocess.run([
        "ffmpeg", "-nostdin", "-v", "error", "-threads", "2", "-framerate", str(FPS),
        "-pattern_type", "glob", "-i", str(frames / "source-*.jpg"), "-filter_threads", "2",
        "-vf", "scale=160:160:force_original_aspect_ratio=decrease,scdet=threshold=12,"
        "metadata=mode=print:key=lavfi.scd.time:file=-", "-f", "null", "-",
    ], check=True, capture_output=True, text=True, timeout=2700)
    cuts = []
    for line in result.stdout.splitlines():
        if line.startswith("frame:"):
            # The selected frame's PTS is its index in the image input at FPS.
            index = int(line.split()[1].removeprefix("pts:"))
            if 0 < index < len(frame_files):
                cuts.append(int(frame_files[index].stem.removeprefix("source-")))
    output.write_text(json.dumps(cuts))
    return len(cuts)


def process(job):
    route = "/" + job["session"] + "/" + job["clip"]
    lease = job["lease"]
    stop = Event()
    thread = Thread(target=heartbeat, args=(route, lease, stop), daemon=True)
    thread.start()
    try:
        with TemporaryDirectory(prefix="rink-tracking-") as folder:
            work = Path(folder)
            source = work / "source.mp4"
            frames = work / "frames"
            output = work / "tracks.mp4"
            detections = work / "detections.ndjson"
            tracks = work / "tracks.ndjson"
            with request(route + "/source", lease=lease) as transfer, source.open("wb") as target:
                shutil.copyfileobj(transfer, target, length=1024 * 1024)
            extract_frames(source, frames)
            shot_args = []
            if job.get("split_shots"):
                cuts = work / "cuts.json"
                print(f"Detected {detect_camera_cuts(frames, cuts)} camera cuts for {job['clip']}", flush=True)
                shot_args = ["--shot-cuts", cuts]
            with detections.open("wb") as boxes:
                subprocess.run([sys.executable, ROOT / "detect-yolox.py", ROOT / "yolox_s.onnx", frames],
                               stdout=boxes, check=True, timeout=2700)
            subprocess.run([ROOT / "rink-track-preview", "--detections", frames, source.stem, str(FPS),
                            os.environ["RINK_FOOTAGE_FONT"], output, detections, tracks, *shot_args, "--interactive"], check=True, timeout=2700)
            with tracks.open("rb") as data, request(route + "/data", "PUT", lease, data, "application/x-ndjson"):
                pass
            with output.open("rb") as video, request(route + "/result", "PUT", lease, video, "video/mp4"):
                pass
        print(f"Published tracking preview for {job['clip']}", flush=True)
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        print(f"Tracking failed for {job['clip']}: {error}", file=sys.stderr, flush=True)
        with request(route + "/failure", "POST", lease):
            pass
    finally:
        stop.set()
        thread.join()


def import_youtube(job):
    route = "/youtube/" + job["id"]
    lease = job["lease"]
    url = "https://www.youtube.com/watch?v=" + job["video"]
    try:
        with TemporaryDirectory(prefix="rink-youtube-") as folder:
            video = Path(folder) / "video.mp4"
            result = subprocess.run([
                "yt-dlp", "--ignore-config", "--no-playlist", "--quiet", "--no-warnings",
                "--socket-timeout", "15", "--retries", "2",
                "--match-filters", "!is_live & duration <= 900",
                "--max-filesize", "200M", "--format", "bv[height<=720][ext=mp4][vcodec^=avc1]/b[height<=720][ext=mp4][vcodec^=avc1]",
                "--print", "after_move:%(title)j",
                "--output", str(video), "--", url,
            ], capture_output=True, text=True, check=True, timeout=210)
            if not video.is_file() or not result.stdout.strip():
                raise ValueError("Expected a public YouTube video up to 15 minutes and 200 MB")
            title = json.loads(result.stdout.strip())
            with video.open("rb") as body, request(route + "/video", "PUT", lease, body, "video/mp4", title):
                pass
        print(f"Imported YouTube video {job['video']}", flush=True)
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        reason = error.stderr.strip() if isinstance(error, subprocess.CalledProcessError) else str(error)
        print(f"YouTube import failed for {job['video']}: {reason}", file=sys.stderr, flush=True)
        failure = None
        if isinstance(error, HTTPError):
            failure = {413: "size", 507: "storage"}.get(error.code)
        elif isinstance(error, ValueError):
            failure = "size"
        with request(route + "/failure", "POST", lease, failure=failure):
            pass


def main():
    while True:
        try:
            with request("/claim", "POST") as response:
                job = json.load(response) if response.status == 200 else None
            if job:
                process(job)
                continue
            with request("/youtube/claim", "POST") as response:
                youtube = json.load(response) if response.status == 200 else None
            if youtube:
                import_youtube(youtube)
                continue
        except (OSError, ValueError) as error:
            print(f"Worker request failed: {error}", file=sys.stderr, flush=True)
            time.sleep(12)
            continue
        time.sleep(4)


if __name__ == "__main__":
    main()
