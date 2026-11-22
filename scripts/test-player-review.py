"""Run with uv run --with playwright --with pillow python scripts/test-player-review.py PREVIEW.mp4.

Requires Google Chrome and the web-import build of target/debug/rink-footage-web.
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
            print(json.dumps(dict(source_ms=ms, width=1280, height=720, tracks=tracks)), file=stream)
    shutil.copyfile(video, directory / "tracks.mp4")
    os.link(directory / "tracks.mp4", directory / "source.mp4")
    Image.new("RGB", (128, 72)).save(directory / "preview.jpg")
    return session, clip


def exercise(page, origin, session, clip):
    field = lambda name: page.locator("#" + name)
    page.goto(f"{origin}/footage/?session={session}")
    field("track-review-" + clip).click()
    expect(field("tracking-play")).to_be_enabled()
    field("tracking-viewport").focus()
    scroll = page.evaluate("window.scrollY")
    page.keyboard.press("Space")
    expect(field("tracking-play")).to_have_text("Pause")
    page.keyboard.press("Space")
    expect(field("tracking-play")).to_have_text("Play")
    assert page.evaluate("window.scrollY") == scroll
    field("tracking-zoom").press("End")
    expect(field("tracking-scale")).to_have_text("400%")
    field("tracking-reset").click()
    expect(field("tracking-scale")).to_have_text("100%")
    for width, height in [(1280, 720), (390, 844)]:
        page.set_viewport_size(dict(width=width, height=height))
        field("tracking-review").evaluate("e => e.scrollIntoView({block:'start'})")
        expect(field("pick-study")).to_be_in_viewport(ratio=1)
        expect(page.locator("#play-review summary")).to_be_in_viewport(ratio=1)
        assert field("tracking-reset").evaluate("""e => {
            const control = e.getBoundingClientRect();
            const frame = document.querySelector('#tracking-video').getBoundingClientRect();
            return control.right <= frame.right && control.bottom <= frame.bottom &&
                   control.left >= frame.left && control.top >= frame.top;
        }""")
    page.set_viewport_size(dict(width=1280, height=720))
    field("study-player").select_option("2")
    field("study-name").fill("Reference")
    field("study-name").press("Space")
    expect(field("study-name")).to_have_value("Reference ")
    expect(field("tracking-play")).to_have_text("Play")
    field("identity-label").click()
    expect(field("identity-status")).to_have_text("Saved.")
    field("study-player").select_option("1")
    page.locator("#study-player-details summary").click()
    field("study-team").fill("Blue")
    field("study-jersey").fill("09")
    field("identity-label").click()
    expect(field("study-name")).to_have_value("Blue #09")
    player = field("study-player").input_value()

    for team, jersey, alias in [("Red", "19", ""), ("Green", "07", "Sam")]:
        held = []

        def hold_response(route):
            held.append((route, route.fetch()))

        page.route("**/identity/suggestions", hold_response)
        with page.expect_request("**/identity/suggestions"):
            field("identity-suggest").click()
        expect(field("study-team")).to_be_enabled()
        field("study-team").fill(team)
        field("study-jersey").fill(jersey)
        if alias:
            field("study-name").fill(alias)
        route, response = held.pop()
        assert response.ok and response.json()["candidates"]
        route.fulfill(response=response)
        page.unroute("**/identity/suggestions", hold_response)
        expect(field("identity-unsure")).to_be_visible()
        expect(field("study-team")).to_have_value(team)
        expect(field("study-jersey")).to_have_value(jersey)
        expect(field("study-name")).to_have_value(alias or "Blue #09")
        with page.expect_response(lambda r: r.url.endswith("/identity") and r.request.method == "POST") as saved:
            field("identity-rename").click()
        assert saved.value.ok
        page.reload()
        field("track-review-" + clip).click()
        field("study-player").select_option(player)
        expect(field("study-name")).to_have_value(alias or f"{team} #{jersey}")
        expect(field("study-team")).to_have_value(team)
        expect(field("study-jersey")).to_have_value(jersey)
        if not page.locator("#study-player-details").evaluate("e => e.open"):
            page.locator("#study-player-details summary").click()


def main(video):
    with tempfile.TemporaryDirectory() as temp:
        session, clip = fixture(Path(temp), video)
        with socket.socket() as available:
            available.bind(("127.0.0.1", 0))
            bind = f"127.0.0.1:{available.getsockname()[1]}"
        origin = "http://" + bind
        env = dict(os.environ, RINK_FOOTAGE_DIR=temp, RINK_FOOTAGE_ORIGIN=origin, RINK_FOOTAGE_BIND=bind)
        server = subprocess.Popen(["target/debug/rink-footage-web"], env=env)
        try:
            for _ in range(100):
                try:
                    with urlopen(origin + "/footage/api/health", timeout=1):
                        break
                except OSError:
                    if server.poll() is not None:
                        raise
                    time.sleep(0.05)
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel="chrome")
                exercise(browser.new_page(), origin, session, clip)
                browser.close()
        finally:
            server.terminate()
            server.wait()


if __name__ == "__main__":
    main(sys.argv[1])
    print("Player review passed for generated and custom labels.")
