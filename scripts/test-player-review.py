"""Drive the footage workspace in a real browser against a throwaway session.

Run from the repo root:

    uv run --with playwright --with pillow python scripts/test-player-review.py PREVIEW.mp4

Requires Google Chrome, `npm ci` in web/, the generated wasm bindings (scripts/build-wasm.sh)
and the web-import build of target/debug/rink-footage-web.
Supply a browser-playable MP4 at least two seconds long.
"""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from urllib.request import urlopen
from uuid import UUID

from PIL import Image
from playwright.sync_api import expect, sync_playwright

REPO = Path(__file__).resolve().parent.parent


def fixture(root, video):
    session, clip, run = [str(UUID(int=n)) for n in (1, 2, 3)]
    directory = root / session / clip
    directory.mkdir(parents=True)
    metadata = dict(id=clip, name="Review fixture", duration_ms=2000, preview_ms=0, container="mp4")
    (directory.parent / "session.json").write_text(json.dumps(dict(
        id=session, version=1, created_ms=1760000000000, clips=[metadata])))
    (directory / "tracking.json").write_text(json.dumps(dict(token=run, until=0, attempts=1, failed=False)))
    tracks = [dict(id=n, predicted=False, xyxy=[100*n, 100, 100*n+70, 225],
                   gear=[[0, 20, 0, 0, 0]] * 3) for n in (1, 2)]
    with (directory / "tracks.ndjson").open("w") as stream:
        for ms in range(0, 2000, 250):
            print(json.dumps(dict(interactive=True, source_ms=ms, width=1280, height=720, tracks=tracks)), file=stream)
    shutil.copyfile(video, directory / "tracks.mp4")
    os.link(directory / "tracks.mp4", directory / "source.mp4")
    Image.new("RGB", (128, 72)).save(directory / "preview.jpg")
    return session, clip


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def wait_for(url):
    for _ in range(100):
        try:
            urlopen(url).close()
            return
        except OSError:
            time.sleep(0.1)
    raise SystemExit(f"{url} did not start")


def exercise(page, origin, session, clip):
    # A link from before the workspace moved into the web app still opens the session.
    page.goto(f"{origin}/footage/?session={session}")
    expect(page).to_have_url(f"{origin}/footage/{session}/{clip}")
    # So does a link from when the app routed by hash.
    page.goto(f"{origin}/#/footage/{session}")
    expect(page).to_have_url(f"{origin}/footage/{session}/{clip}")
    # A reload of a deep link lands on the same page.
    page.reload()
    expect(page).to_have_url(f"{origin}/footage/{session}/{clip}")
    play = page.get_by_role("button", name="Play", exact=True)
    expect(play).to_be_visible()

    # The page itself never scrolls; only the side panels do.
    assert page.evaluate("document.documentElement.scrollHeight <= innerHeight")

    page.keyboard.press("Space")
    expect(page.get_by_role("button", name="Pause", exact=True)).to_be_visible()
    page.keyboard.press("Space")
    expect(play).to_be_visible()

    # Selecting a skater shows them in the inspector and on the timeline.
    page.get_by_role("button", name="S01").click()
    expect(page.locator(".who strong")).to_have_text("S01 · blue")
    expect(page.locator("[data-lane=selection] .seen")).to_have_count(1)

    # Naming the skater moves them from the unnamed tracks to the players.
    page.get_by_label("Name", exact=True).fill("Anna")
    page.get_by_label("Jersey").fill("7")
    page.get_by_role("button", name="Confirm new player").click()
    expect(page.locator(".who strong")).to_have_text("Anna")
    expect(page.get_by_role("button", name="S01")).to_have_count(0)

    page.get_by_label("This is me").check()
    expect(page.locator("[data-lane=you] .seen")).to_have_count(1)

    # P starts a play, a number picks its label, and saving puts a bar on the Plays lane.
    page.keyboard.press("p")
    page.keyboard.press("1")
    page.get_by_role("button", name="Save play").click()
    expect(page.locator("[data-lane=plays] .bar:not(.draft)")).to_have_text("Get open")

    # Escape clears the selection.
    page.keyboard.press("Escape")
    expect(page.locator(".who")).to_have_count(0)

    # Links change the page without reloading it, and Back returns.
    page.get_by_role("link", name="Footage", exact=True).click()
    expect(page).to_have_url(f"{origin}/footage")
    expect(page.get_by_role("link", name="Review fixture")).to_be_visible()
    page.go_back()
    expect(page).to_have_url(f"{origin}/footage/{session}/{clip}")
    page.get_by_role("link", name="Lessons").click()
    expect(page).to_have_url(f"{origin}/")


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    service_port, web_port = free_port(), free_port()
    origin = f"http://localhost:{web_port}"
    with tempfile.TemporaryDirectory() as folder:
        session, clip = fixture(Path(folder), sys.argv[1])
        service = subprocess.Popen(
            [REPO / "target/debug/rink-footage-web"],
            env=dict(os.environ, RINK_FOOTAGE_DIR=folder, RINK_FOOTAGE_ORIGIN=origin,
                     RINK_FOOTAGE_BIND=f"127.0.0.1:{service_port}"))
        web = subprocess.Popen(
            ["npx", "vite", "--port", str(web_port), "--strictPort"], cwd=REPO / "web",
            env=dict(os.environ, RINK_FOOTAGE_DEV=f"http://127.0.0.1:{service_port}"))
        try:
            wait_for(f"http://127.0.0.1:{service_port}/api/footage/health")
            wait_for(origin)
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome")
                page = browser.new_page(viewport=dict(width=1440, height=860))
                exercise(page, origin, session, clip)
                browser.close()
        finally:
            web.terminate()
            service.terminate()
    print("ok")


if __name__ == "__main__":
    main()
