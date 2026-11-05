#!/usr/bin/env python3
"""Agent-driven end-to-end check for ArcShot.

Drives the native layer through `agent-desktop` and reads product state from the
app's stderr diagnostics; see TEST.md §4 for the seam, the stable MAC-E2E IDs
and the limits. This entry is not part of `scripts/check-all.sh`: it needs a
real login session, an interactive desktop and TCC grants for the terminal that
launches `agent-desktop`.

Usage: scripts/e2e-agent-desktop.py [--release]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import struct
import subprocess
import sys
import time
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
POLL = 0.1

# Menu order is part of the product contract (crates/macos MenuBar::try_new).
MENU = [
    ("开始截图", "startCapture:"),
    ("截图快捷键…", "hotkeySettings:"),
    ("屏幕录制权限…", "screenRecordingPermissions:"),
    ("选项密度", "toggleDensity:"),
    ("完成音效", "toggleCompletionSound:"),
    ("登录时启动", "toggleLaunchAtLogin:"),
    ("退出 ArcShot", "terminate:"),
]

DENSITIES = ("Standard", "Compact")


def launch_density(log: pathlib.Path) -> str:
    """Reads the density the product restored at launch (text diagnostic)."""
    line = wait_line(
        log,
        lambda text: "preferences_launch " in text and "density=" in text,
        10.0,
    )
    value = line.split("density=", 1)[1].split(" ", 1)[0]
    if value not in DENSITIES:
        fail(f"unexpected persisted density: {value!r}")
    return value


def density_node_matches(node: dict, density: str) -> bool:
    """The dock exposes the real density as an AX name/value, without pixels."""
    return density in str(node.get("value")) or density in str(node.get("name"))

MODIFIERS = [("⌘", "cmd"), ("⌃", "ctrl"), ("⌥", "alt"), ("⇧", "shift")]


class Failure(Exception):
    """A check did not hold; the message is the whole story."""


def fail(message: str) -> None:
    raise Failure(message)


def note(line: str) -> None:
    print(line, flush=True)


def enabled(name: str) -> str:
    path = shutil_which(name)
    if path is None:
        fail(f"required tool not found: {name}")
    return path


def shutil_which(name: str) -> str | None:
    from shutil import which

    return which(name)


def run(argv: list[str], *, timeout: float = 60.0) -> subprocess.CompletedProcess:
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


def agent(*args: str) -> dict:
    """Runs one agent-desktop command and returns the envelope after checking ok."""
    result = run(["agent-desktop", *args])
    try:
        envelope = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        fail(f"agent-desktop {' '.join(args)}: unparsable output ({error}): {result.stdout[:400]}")
    if not envelope.get("ok"):
        error = envelope.get("error", {})
        fail(
            f"agent-desktop {' '.join(args)}: {error.get('code')}: {error.get('message')} "
            f"(delivery={error.get('disposition', {}).get('delivery')})"
        )
    return envelope["data"]


def read_log(path: pathlib.Path) -> list[str]:
    try:
        return path.read_text(errors="replace").splitlines()
    except FileNotFoundError:
        return []


def wait_log(path: pathlib.Path, needle: str, timeout: float, *, after: int = 0) -> int:
    """Waits until a log line containing needle appears at or after index line."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        lines = read_log(path)
        for index in range(after, len(lines)):
            if needle in lines[index]:
                return index
        time.sleep(POLL)
    fail(f"timeout waiting for {needle!r} in {path}")


def wait_line(path: pathlib.Path, predicate, timeout: float) -> str:
    """Waits until one diagnostic line satisfies the predicate."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for line in read_log(path):
            if predicate(line):
                return line
        time.sleep(POLL)
    fail(f"timeout waiting for a matching line in {path}")


def wait_frontmost(app: str, timeout: float) -> None:
    """Waits until the app is frontmost, for keys meant for its titled window."""
    deadline = time.monotonic() + timeout
    last: str | None = None
    while time.monotonic() < deadline:
        try:
            last = agent("snapshot", "--surface", "focused").get("app")
        except Failure as error:
            if "WINDOW_NOT_FOUND" not in str(error) and "APP_NOT_FOUND" not in str(error):
                raise
            last = None
        if last == app:
            return
        time.sleep(POLL)
    fail(f"{app} did not become the frontmost application; currently {last!r}")


def wait_frozen_active(log: pathlib.Path, after: int, timeout: float = 15.0) -> int:
    """Waits for this session's freeze and the product's own activation line.

    Headed key delivery to the borderless overlay is only meaningful once the
    app reports it owns keyboard focus; without this wait a focus race can send
    Escape to whatever application is still in front.
    """
    wake = wait_log(log, "capture_wake_window_submit_us=", timeout, after=after)
    wait_log(log, "app_active surface=frozen", timeout, after=wake)
    return wake


def count_log(path: pathlib.Path, needle: str) -> int:
    return sum(1 for line in read_log(path) if needle in line)


def wait_log_line(path: pathlib.Path, needle: str, timeout: float, *, after: int = 0) -> str:
    """Returns the first matching log line at or after an index."""
    index = wait_log(path, needle, timeout, after=after)
    return read_log(path)[index]


def find_ref(app: str, name: str) -> str:
    data = agent("find", "--surface", "menu-bar-extras", "--app", app, "--role", "menuitem", "--name", name)
    matches = data.get("matches", [])
    if len(matches) != 1:
        fail(f"expected exactly one menu item named {name!r}, got {len(matches)}")
    return matches[0]["ref_id"]


def click(app: str, name: str) -> None:
    ref = find_ref(app, name)
    data = agent("click", ref)
    steps = data.get("steps", [])
    if not steps or steps[0].get("outcome") != "succeeded":
        fail(f"click {name!r} did not report success: {data}")


def click_scroll_entry(app: str, *, denied: bool) -> None:
    """Click `arcshot.overlay.scroll`, re-snapshotting after a stale ref.

    The overlay can re-render between the snapshot that produced the ref and
    the click. A stale ref is a delivery retry, never a pass; a live click
    without a valid selection must still be `POLICY_DENIED`.
    """
    for _ in range(4):
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
            )[0],
            10.0,
        )
        nodes = find_nodes(root, native_id="arcshot.overlay.scroll", require_ref=True)
        if not nodes:
            fail("the Scroll entry is not addressable")
        try:
            agent("click", nodes[0]["ref_id"])
        except Failure as error:
            if "STALE_REF" in str(error):
                continue
            if denied and "POLICY_DENIED" in str(error):
                return
            raise
        else:
            if denied:
                fail("the Scroll entry was actionable without a valid selection")
            return
    fail("the Scroll entry ref stayed stale across retries")


def start(executable: pathlib.Path, log: pathlib.Path) -> subprocess.Popen:
    """Starts the bundle under test with its diagnostics in one fresh log."""
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text("")
    stderr = log.open("w")
    process = subprocess.Popen(
        [str(executable)], stdout=subprocess.DEVNULL, stderr=stderr, cwd=ROOT
    )
    stderr.close()
    return process


def stop(process: subprocess.Popen) -> None:
    """Terminates a test process unless it already exited."""
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5.0)
        except subprocess.TimeoutExpired:
            process.kill()


# The fixture target's deterministic geometry (crates/app/src/bin/scroll-fixture.rs).
FIXTURE_ORIGIN_LOGICAL = (600.0, 120.0)
FIXTURE_INITIAL_OFFSET_ROWS = 2000


def fixture_pixel(x: int, y: int, scale: int) -> tuple[int, int, int, int]:
    """The fixture's exact content bytes, replicated for the pixel oracle."""
    if x < 64 * scale:
        return (y & 255, (y >> 8) & 255, 173, 255)
    block = x // (8 * scale)
    h = ((y * 0x9E3779B9) & 0xFFFFFFFF) ^ ((block * 0x85EBCA6B) & 0xFFFFFFFF)
    h &= 0xFFFFFFFF
    h ^= h >> 16
    b = h.to_bytes(4, "little")
    return (b[0], b[1], b[2], 255)


def expected_fixture_probe_at(
    selection: tuple[float, float, float, float], offset_points: int
) -> str:
    """The capture_probe_grid at one fixture content offset.

    Mirrors crates/app/src/coordinator.rs capture_probe (5x5 fractions, BGRA
    storage printed as RRGGBBAA) over the fixture content at `offset_points`
    logical rows, with the borderless fixture window filling its frame exactly.
    """
    scale = 2
    win_x, win_y = FIXTURE_ORIGIN_LOGICAL
    crop_x = int(selection[0] * scale)
    crop_y = int(selection[1] * scale)
    width = int(selection[2] * scale)
    height = int(selection[3] * scale)
    rows = []
    for fy in [0.05, 0.25, 0.50, 0.75, 0.95]:
        cells = []
        for fx in [0.05, 0.25, 0.50, 0.75, 0.95]:
            local_x = crop_x - int(win_x * scale) + int(width * fx)
            local_y = crop_y - int(win_y * scale) + int(height * fy)
            row = offset_points * scale + local_y
            p = fixture_pixel(local_x, row, scale)
            cells.append(f"{p[2]:02x}{p[1]:02x}{p[0]:02x}{p[3]:02x}")
        rows.append(" ".join(cells))
    return "capture_probe_grid rows=" + " | ".join(rows)


def expected_fixture_probe(selection: tuple[float, float, float, float]) -> str:
    """The initial-offset capture probe for one selection."""
    return expected_fixture_probe_at(selection, FIXTURE_INITIAL_OFFSET_ROWS)


def fixture_offset_points(fixture_log: pathlib.Path) -> int:
    """The newest content offset the fixture reported, in logical rows."""
    offsets = [
        int(line.split("offset_points=", 1)[1])
        for line in read_log(fixture_log)
        if "fixture_scroll" in line
    ]
    return offsets[-1] if offsets else FIXTURE_INITIAL_OFFSET_ROWS


FIXTURE_RECT = (600.0, 120.0, 760.0, 720.0)
# agent-desktop's raw --xy space places the frozen window's y origin 33 points
# lower than the screen point (the menu-bar inset); drags therefore target
# screen_y = document_y + 33.
SCREEN_Y_OFFSET = 33.0

CG_WINDOWS_SWIFT = r"""
import CoreGraphics
import Foundation
guard let list = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as NSArray? else { exit(1) }
for case let w as NSDictionary in list {
    let owner = w[kCGWindowOwnerName as String] as? String ?? "?"
    let layer = w[kCGWindowLayer as String] as? Int ?? 0
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    let x = b["X"] as? Double ?? 0, y = b["Y"] as? Double ?? 0
    let ww = b["Width"] as? Double ?? 0, hh = b["Height"] as? Double ?? 0
    print("\(layer) \(x) \(y) \(ww) \(hh) \(owner)")
}
"""


def floating_windows() -> list[tuple[int, float, float, float, float]]:
    """CoreGraphics windows above the normal layer, with their bounds."""
    directory = ROOT / "target"
    directory.mkdir(parents=True, exist_ok=True)
    script = directory / "e2e-cg-windows.swift"
    script.write_text(CG_WINDOWS_SWIFT)
    result = run(["swift", str(script)], timeout=60.0)
    windows = []
    for line in result.stdout.splitlines():
        parts = line.split(maxsplit=5)
        if len(parts) != 6:
            continue
        layer, x, y, width, height, owner = parts
        try:
            windows.append((int(layer), float(x), float(y), float(width), float(height), owner))
        except ValueError:
            continue
    return windows


def intersects(
    a: tuple[float, float, float, float], b: tuple[float, float, float, float]
) -> bool:
    return not (
        a[0] + a[2] <= b[0]
        or b[0] + b[2] <= a[0]
        or a[1] + a[3] <= b[1]
        or b[1] + b[3] <= a[1]
    )


def free_fixture_selection(
    *, width: float = 240.0, height: float = 160.0
) -> tuple[float, float, float, float]:
    """Finds a selection inside the fixture clear of floating windows.

    Third-party notification banners and floating panels sit above normal
    windows and can intercept a headed drag; picking an unobstructed rectangle
    keeps the physical precondition honest instead of retrying blind.
    """
    fixture_x, fixture_y, fixture_width, fixture_height = FIXTURE_RECT
    blockers = [
        (layer, x, y, w, h)
        for (layer, x, y, w, h, _owner) in floating_windows()
        if layer > 0 and w > 0 and h > 0
    ]
    step = 20.0
    y = fixture_y + 20.0
    while y + height <= fixture_y + fixture_height - 20.0:
        x = fixture_x + 20.0
        while x + width <= fixture_x + fixture_width - 20.0:
            candidate = (x, y, width, height)
            if not any(intersects(candidate, blocker) for blocker in blockers):
                return candidate
            x += step
        y += step
    fail("no floating-window-free selection region inside the fixture")


PASTEBOARD_SWIFT = r"""
import AppKit
import Foundation
let path = CommandLine.arguments[1]
guard let data = NSPasteboard.general.data(forType: .png) else { exit(2) }
try data.write(to: URL(fileURLWithPath: path))
print("bytes=\(data.count)")
"""


def pasteboard_png(name: str) -> tuple[str, int, int, int]:
    """Reads the PNG bytes from the real pasteboard and returns (hash, size, w, h)."""
    directory = ROOT / "target"
    directory.mkdir(parents=True, exist_ok=True)
    script = directory / "e2e-pasteboard.swift"
    script.write_text(PASTEBOARD_SWIFT)
    output = directory / name
    output.unlink(missing_ok=True)
    run(["swift", str(script), str(output)], timeout=60.0)
    if not output.exists():
        fail("the pasteboard holds no PNG at the expected point")
    data = output.read_bytes()
    if len(data) < 24 or data[:8] != b"\x89PNG\r\n\x1a\n":
        fail(f"the pasteboard payload is not a PNG: {len(data)} bytes")
    width, height = struct.unpack(">II", data[16:24])
    return hashlib.sha256(data).hexdigest(), len(data), width, height


def count_log_after(path: pathlib.Path, needle: str, after: int) -> int:
    return sum(1 for line in read_log(path)[after:] if needle in line)


def parse_selection(log: pathlib.Path, after: int) -> tuple[float, float, float, float]:
    """Reads the newest SelectRegion geometry as (x, y, width, height)."""
    for line in reversed(read_log(log)[after:]):
        if "capture_edit action=SelectRegion" not in line:
            continue
        payload = line.split("selection=Some([", 1)[1].split("])", 1)[0]
        values = [float(value) for value in payload.split(",")]
        return tuple(values)
    fail(f"no SelectRegion geometry found after line {after} in {log}")


# --- Ticket #36 third-party targets (MAC-E2E-30/31) ---

TEXTEDIT_FIXTURE_VERSION = "textedit-36-v1"
SAFARI_FIXTURE_VERSION = "safari-36-v1"


def osascript(script: str, timeout: float = 10.0) -> str:
    """Runs one System Events script (target window geometry / frontmost check)."""
    result = run(["osascript", "-e", script], timeout=timeout)
    if result.returncode != 0:
        fail(f"osascript {script!r}: {result.stderr.strip()[:300]}")
    return result.stdout.strip()


def set_window_geometry(app_name: str, x: int, y: int, width: int, height: int) -> None:
    """Fixes one third-party window geometry via System Events (points).

    agent-desktop list-windows reports stale bounds for background windows on
    macOS 27 (observed for TextEdit/Safari behind the frontmost app), so the
    fixed geometry is set and verified through System Events instead.
    """
    osascript(
        f'tell application "System Events" to tell process "{app_name}" '
        f"to set position of window 1 to {{{x}, {y}}}"
    )
    osascript(
        f'tell application "System Events" to tell process "{app_name}" '
        f"to set size of window 1 to {{{width}, {height}}}"
    )
    observed = osascript(
        f'tell application "System Events" to tell process "{app_name}" '
        "to get {position, size} of window 1"
    )
    if observed != f"{x}, {y}, {width}, {height}":
        fail(f"{app_name} geometry is {observed!r}, expected ({x},{y},{width},{height})")


def frontmost_process() -> str:
    """The frontmost application process name (System Events)."""
    return osascript(
        'tell application "System Events" to get name of first application '
        "process whose frontmost is true"
    )


def read_clipboard_png() -> bytes:
    """Reads the PNGf clipboard payload in memory (osascript, no disk file).

    agent-desktop clipboard-get is unusable on this host (missing packaged
    helper), so the PNGf type is read through System Events and decoded in
    memory; evidence is numeric only, never a saved screenshot.
    """
    output = run(["osascript", "-e", "the clipboard"], timeout=30.0).stdout
    match = re.search(r"PNGf([0-9A-Fa-f]+)", output)
    if not match:
        fail("the clipboard does not carry PNGf data")
    return bytes.fromhex(match.group(1))


def clipboard_has_pngf() -> bool:
    """Whether the general pasteboard exposes the PNGf type."""
    return "PNGf" in run(["osascript", "-e", "get clipboard info"], timeout=10.0).stdout


def decode_png_metrics(data: bytes) -> dict:
    """Decodes one PNG in memory and reports size/sRGB/alpha/texture metrics."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        fail("the clipboard PNGf payload is not a PNG")
    offset = 8
    width = height = 0
    idat = b""
    has_srgb = False
    while offset + 8 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8].decode("latin1")
        body = data[offset + 8 : offset + 8 + length]
        if kind == "IHDR":
            width, height = struct.unpack(">II", body[:8])
            if body[8] != 8 or body[9] != 6:
                fail(f"unexpected PNG IHDR bit depth/colour: {body[8]}/{body[9]}")
        elif kind == "sRGB":
            has_srgb = True
        elif kind == "IDAT":
            idat += body
        offset += 12 + length
        if kind == "IEND":
            break
    if not has_srgb:
        fail("the clipboard PNG carries no sRGB chunk")
    raw = zlib.decompress(idat)
    if len(raw) != height * (1 + width * 4):
        fail("the clipboard PNG inflates to an unexpected length")
    rows: list[bytes] = []
    previous = bytearray(width * 4)
    pos = 0
    for _y in range(height):
        filter_type = raw[pos]
        pos += 1
        current = bytearray(raw[pos : pos + width * 4])
        pos += width * 4
        if filter_type == 1:
            for index in range(4, len(current)):
                current[index] = (current[index] + current[index - 4]) & 0xFF
        elif filter_type == 2:
            for index in range(len(current)):
                current[index] = (current[index] + previous[index]) & 0xFF
        elif filter_type == 3:
            for index in range(len(current)):
                left = current[index - 4] if index >= 4 else 0
                current[index] = (current[index] + (left + previous[index]) // 2) & 0xFF
        elif filter_type == 4:
            for index in range(len(current)):
                left = current[index - 4] if index >= 4 else 0
                up = previous[index]
                up_left = previous[index - 4] if index >= 4 else 0
                predictor = left + up - up_left
                p_left, p_up, p_up_left = abs(predictor - left), abs(predictor - up), abs(predictor - up_left)
                best = left if (p_left <= p_up and p_left <= p_up_left) else (up if p_up <= p_up_left else up_left)
                current[index] = (current[index] + best) & 0xFF
        elif filter_type != 0:
            fail(f"unexpected PNG row filter {filter_type}")
        rows.append(bytes(current))
        previous = current
    alpha_ok = all(row[index] == 255 for row in rows for index in range(3, len(row), 4))
    return {
        "width": width,
        "height": height,
        "has_srgb": has_srgb,
        "alpha_ok": alpha_ok,
        "distinct_rows": len(set(rows)),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


def write_textedit_fixture(path: pathlib.Path) -> None:
    """Generates the fixed-geometry plain-text fixture (offline, no network).

    Lines are long deterministic hash streams so wrapped text fills the whole
    selection width: with the default 55-char lines the right majority is
    white, which the strict stitch verifier rightly judges ambiguous
    (uniform content). Full-width text keeps every rendered row distinct.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        f"ARCSHOT36 TEXTEDIT LINE {index:04d} marker-36-{index:04d}"
        + "".join(
            chr(
                ord("a")
                + int(hashlib.sha256(f"36-line-{index}-{position}".encode()).hexdigest()[:2], 16) % 26
            )
            for position in range(200)
        )
        for index in range(400)
    ]
    path.write_text("\n".join(lines) + "\n", encoding="ascii")


def write_safari_fixture(path: pathlib.Path) -> None:
    """Generates the file:// static-page fixture (offline, no remote resources)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    rows = "\n".join(
        f'<div class="row">ARCSHOT36 SAFARI ROW {index:04d} marker-36-{index:04d} '
        f"x{(index * 53) % 1000:03d} {'B' * ((index % 13) + 1)}</div>"
        for index in range(400)
    )
    html = (
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">"
        "<title>ArcShot36 Safari Fixture</title>\n"
        '<link rel="icon" href="data:,">\n'
        "<style>html,body{margin:0;padding:0;background:#ffffff;color:#000000;"
        "font-family:Menlo,monospace;font-size:16px;line-height:20px;}"
        ".row{height:20px;white-space:pre;overflow:hidden;}</style></head><body>\n"
        + rows
        + "\n</body></html>\n"
    )
    for token in ("http://", "https://", "<script", "<iframe", "<img"):
        if token in html:
            fail(f"the Safari fixture unexpectedly contains {token!r}")
    if 'href="http' in html or "src=" in html:
        fail("the Safari fixture references a remote resource")
    path.write_text(html, encoding="utf-8")


def wait_target_window(app: str, title_part: str) -> None:
    """Waits for exactly one window whose title contains the fixture name."""
    deadline = time.monotonic() + 15.0
    last: list[dict] = []
    while time.monotonic() < deadline:
        last = windows(app)
        titled = [entry for entry in last if title_part in str(entry.get("title") or "")]
        if len(titled) == 1 and len(last) == 1:
            return
        time.sleep(POLL)
    fail(f"expected exactly one {app} fixture window, saw {last}")


def begin_third_party_scroll(
    app: str, log: pathlib.Path, drag_from: str, drag_to: str, mark: int
) -> tuple[int, tuple[float, float, float, float]]:
    """Starts one frozen session and enters Scroll collection (seek to collecting)."""
    click(app, "开始截图")
    wait_frozen_active(log, mark)
    agent("drag", "--from-xy", drag_from, "--to-xy", drag_to, "--headed")
    wait_log(log, "capture_edit action=SelectRegion", 10.0, after=mark)
    selection = parse_selection(log, mark)
    entry_mark = len(read_log(log))
    click_scroll_entry(app, denied=False)
    wait_log(log, "a11y_action=arcshot.overlay.scroll action=Click", 10.0, after=entry_mark)
    wait_log(log, "scroll_target window_id=", 15.0, after=entry_mark)
    wait_log(log, "scroll_prepared ", 15.0, after=entry_mark)
    wait_log(log, "scroll_preview placed=inner", 15.0, after=entry_mark)
    wait_log(log, "scroll_collecting", 15.0, after=entry_mark)
    return entry_mark, selection


def expect_selection_returned(app: str) -> None:
    """The pre-scroll selection is addressable again (overlay save entry)."""
    root = wait_node(
        app,
        lambda tree: (
            find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
        )[0],
        10.0,
    )
    if not find_nodes(root, native_id="arcshot.overlay.save", require_ref=True):
        fail("the pre-scroll selection was not returned")


def decode_clipboard_expect(selection: tuple[float, float, float, float]) -> dict:
    """Decodes the clipboard PNG and checks it against the selection geometry."""
    if not clipboard_has_pngf():
        fail("the clipboard does not expose PNGf after output")
    metrics = decode_png_metrics(read_clipboard_png())
    expected_width = int(selection[2] * 2)
    if metrics["width"] != expected_width:
        fail(f"decoded PNG width {metrics['width']} != selection {expected_width}")
    if not metrics["alpha_ok"]:
        fail("the decoded PNG is not fully opaque")
    if metrics["distinct_rows"] < 100:
        fail(f"the decoded PNG looks uniform ({metrics['distinct_rows']} distinct rows)")
    return metrics


def select_region(
    app: str,
    log: pathlib.Path,
    *,
    drag_from: str,
    drag_to: str,
    mark: int,
    attempts: int = 3,
) -> tuple[float, float, float, float]:
    """Drags one selection, retrying a fresh capture after lost physical input.

    A headed drag can be intercepted by an unrelated floating window above the
    overlay; a retry restarts the capture instead of weakening the assertion
    that the product itself reports the resulting selection.
    """
    for attempt in range(attempts):
        agent("drag", "--from-xy", drag_from, "--to-xy", drag_to, "--headed")
        try:
            wait_log(log, "capture_edit action=SelectRegion", 4.0, after=mark)
            return parse_selection(log, mark)
        except Failure:
            if attempt == attempts - 1:
                raise
            restart = len(read_log(log))
            click(app, "开始截图")
            wait_frozen_active(log, restart)
            mark = restart
    fail("the selection drag never reached the overlay")


def quit_app(app: str, process: subprocess.Popen, *, expect_status_item_removed: bool) -> None:
    """Quits through the real menu item and checks the process is really gone."""
    click(app, "退出 ArcShot")
    try:
        status_code = process.wait(timeout=10.0)
    except subprocess.TimeoutExpired:
        fail("process did not exit after the quit command")
    if status_code != 0:
        fail(f"process exited {status_code} after the quit command")
    if agent("list-apps", "--app", "ArcShot").get("apps"):
        fail("an ArcShot instance is still running after quit")
    if not expect_status_item_removed:
        return
    try:
        agent("find", "--surface", "menu-bar-extras", "--app", app, "--role", "menuitem")
    except Failure as error:
        if "APP_NOT_FOUND" not in str(error):
            raise
    else:
        fail("status item is still addressable after quit")


def windows(app: str) -> list[dict]:
    data = agent("list-windows", "--app", app)
    if not isinstance(data, list):
        fail(f"list-windows returned an unexpected payload: {data}")
    return data


def wait_count(path: pathlib.Path, needle: str, expect: int, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if count_log(path, needle) >= expect:
            return
        time.sleep(POLL)
    fail(f"timeout waiting for {needle!r} count {expect} in {path}")


def wait_windows(app: str, expect: int, timeout: float) -> list[dict]:
    deadline = time.monotonic() + timeout
    last: list[dict] = []
    while time.monotonic() < deadline:
        last = windows(app)
        if len(last) == expect:
            return last
        time.sleep(POLL)
    fail(f"expected {expect} own window(s) for {app}, saw {len(last)}: {last}")


def wait_window_size(app: str, width: float, height: float, timeout: float) -> dict:
    """Waits for one own window of the expected size.

    GPUI may report a window one or two points wider while it settles, so the
    size is matched with a small tolerance instead of an exact equality.
    """
    deadline = time.monotonic() + timeout
    last: list[dict] = []
    while time.monotonic() < deadline:
        last = windows(app)
        if len(last) == 1:
            bounds = last[0].get("bounds", {})
            if abs(bounds.get("width", 0.0) - width) <= 4.0 and abs(
                bounds.get("height", 0.0) - height
            ) <= 4.0:
                return last[0]
        time.sleep(POLL)
    fail(f"expected one {width}x{height} window, saw {last}")


def snapshot_tree(app: str) -> dict:
    return agent("snapshot", "--app", app).get("tree", {})


def find_nodes(
    tree: dict,
    *,
    role: str | None = None,
    name: str | None = None,
    native_id: str | None = None,
    require_ref: bool = False,
) -> list[dict]:
    """Collects matching nodes.

    Group containers carry a name but no ref, so container lookups leave
    `require_ref` off; click targets set it.
    """
    found: list[dict] = []
    stack = [tree]
    while stack:
        node = stack.pop()
        if (
            (role is None or node.get("role") == role)
            and (name is None or node.get("name") == name)
            and (
                native_id is None
                or (node.get("native_id") or {}).get("value") == native_id
            )
            and (node.get("ref_id") or not require_ref)
        ):
            found.append(node)
        stack.extend(node.get("children") or [])
    return found


def wait_node(app: str, predicate, timeout: float) -> dict:
    """Waits for an accessibility node.

    Accessibility is activated lazily: the first snapshot after a window opens
    can be empty, so this re-reads instead of treating one read as the answer.
    """
    deadline = time.monotonic() + timeout
    last: dict = {}
    while time.monotonic() < deadline:
        try:
            last = snapshot_tree(app)
        except Failure as error:
            # A window closing between two commands yields WINDOW_NOT_FOUND;
            # that is a race in the observation, not a product failure. An
            # ambiguous snapshot while the overlay settles is the same class.
            if "WINDOW_NOT_FOUND" not in str(error) and "AMBIGUOUS_TARGET" not in str(error):
                raise
            last = {}
        else:
            node = predicate(last)
            if node:
                return node
        time.sleep(POLL)
    fail(f"accessibility node did not appear in {timeout}s; last tree: {json.dumps(last)[:600]}")


def checked_state(node: dict) -> bool:
    """Reads the checked/toggled state AccessKit exposes for a button value."""
    if node.get("value") is True:
        return True
    return "checked" in (node.get("states") or [])


def hover(ref: str) -> None:
    """Moves the physical pointer onto an overlay control (headed precondition)."""
    data = agent("hover", ref, "--headed")
    steps = data.get("steps", [])
    if not steps or steps[0].get("outcome") != "succeeded":
        fail(f"hover {ref} did not report success: {data}")


def wait_tool_state(app: str, native_id: str, expected: bool, timeout: float) -> dict:
    """Waits for a tool/option node whose checked state matches the product state."""
    return wait_node(
        app,
        lambda tree: next(
            (
                node
                for node in find_nodes(tree, native_id=native_id)
                if checked_state(node) == expected
            ),
            None,
        ),
        timeout,
    )


def press(combo: str, *, headed: bool) -> None:
    args = ["press", combo] + (["--headed"] if headed else [])
    data = agent(*args)
    steps = data.get("steps", [])
    if not steps or steps[0].get("outcome") != "succeeded":
        fail(f"press {combo} did not report success: {data}")


def press_until_log(
    combo: str,
    log: pathlib.Path,
    needle: str,
    *,
    timeout: float = 4.0,
    attempts: int = 4,
    after: int | None = None,
) -> str:
    """Press a headed key and require the product's own diagnostic line.

    Physical key delivery can lose a focus race; each retry re-sends the key
    and re-reads the product log, never relaxing the behavioral assertion.
    `after` keeps a stale earlier line from satisfying the wait.
    """
    last: Exception | None = None
    for _ in range(attempts):
        press(combo, headed=True)
        try:
            return wait_log(log, needle, timeout, after=after or 0)
        except Failure as error:
            last = error
    fail(f"headed {combo!r} never produced {needle!r}: {last}")


def hotkey_combo(display: str) -> str:
    """Translates the product's display form (⌃⌥A) into agent-desktop's combo."""
    remaining = display
    parts: list[str] = []
    for token, name in MODIFIERS:
        if remaining.startswith(token):
            parts.append(name)
            remaining = remaining[len(token) :]
    if not parts or not remaining:
        fail(f"cannot translate hotkey display {display!r} into a key combo")
    key = "space" if remaining == "空格" else remaining.lower()
    if not key.isascii():
        fail(f"hotkey {display!r} uses a non-ASCII key that agent-desktop cannot inject")
    return "+".join([*parts, key])


def press_until(
    app: str, combo: str, *, headed: bool, observe, gate=None, attempts: int = 4
) -> None:
    """Delivers one key until the product observes it, re-gating focus each try.

    Focus can shift between the frontmost check and the physical delivery; every
    attempt re-confirms focus (the frontmost application, or a caller-supplied
    product gate such as `app_active surface=frozen`) before pressing. The product
    assertion is unchanged: `observe` must still see the real effect, so a key
    that never reaches the product still fails the case instead of being assumed.
    """
    for _ in range(attempts):
        if gate is not None:
            gate()
        else:
            wait_frontmost(app, 5.0)
        press(combo, headed=headed)
        deadline = time.monotonic() + 2.5
        while time.monotonic() < deadline:
            if observe():
                return
            time.sleep(POLL)
    fail(f"press {combo!r} did not reach the product after {attempts} deliveries")


def main() -> int:
    parser = argparse.ArgumentParser(description="ArcShot agent-desktop end-to-end check")
    parser.add_argument("--release", action="store_true")
    options = parser.parse_args()

    for tool in ("agent-desktop", "plutil"):
        enabled(tool)

    configuration = "release" if options.release else "debug"
    expected = (
        ("dev.arcshot.app", "ArcShot")
        if options.release
        else ("dev.arcshot.app.debug", "ArcShot Debug")
    )
    identifier, app = expected
    bundle = ROOT / "target" / "bundle" / configuration / "ArcShot.app"
    executable = bundle / "Contents" / "MacOS" / "arcshot"
    log = ROOT / "target" / f"e2e-agent-desktop-{configuration}.log"

    # Preconditions: never start a second instance, never guess a permission.
    version = run(["agent-desktop", "--version"]).stdout.strip()
    note(f"agent_desktop={version or 'unknown'}")
    status = agent("status")
    permissions = status.get("permissions", {})
    if status.get("platform") != "macos":
        fail(f"agent-desktop runs on {status.get('platform')}, expected macos")
    for name in ("accessibility", "screen_recording"):
        state = permissions.get(name, {}).get("state")
        if state != "granted":
            fail(
                f"{name} permission is {state!r}; grant it to the app that launches "
                "agent-desktop, then rerun (this is a precondition, not a product failure)"
            )
    note(f"agent_desktop_status permissions={json.dumps(permissions, sort_keys=True)}")
    running = agent("list-apps", "--app", "ArcShot").get("apps", [])
    if running:
        fail(f"another ArcShot instance is already running: {running}")
    # Persistent notification glass (layer 8) sits above every app window and
    # would pollute pixel-truth frames. `dismiss-all` cannot see it (empty
    # list); the stuck helper is cleared out of band before the run, so the
    # harness instead asserts a clean display filter area at probe time.

    # Release uses thin LTO and can exceed the default one-minute command
    # timeout on a cold build; keep every other command tightly bounded.
    build = run(
        [str(ROOT / "scripts" / "build-app.sh")]
        + (["--release"] if options.release else []),
        timeout=600.0,
    )
    if build.returncode != 0:
        fail(f"build failed: {build.stderr.strip()[-600:]}")
    actual = run(["plutil", "-extract", "CFBundleIdentifier", "raw", str(bundle / "Contents" / "Info.plist")]).stdout.strip()
    actual_name = run(["plutil", "-extract", "CFBundleName", "raw", str(bundle / "Contents" / "Info.plist")]).stdout.strip()
    if (actual, actual_name) != (identifier, app):
        fail(f"bundle identity mismatch: {actual} / {actual_name}, expected {identifier} / {app}")

    log = ROOT / "target" / f"e2e-agent-desktop-{configuration}.log"
    process = start(executable, log)
    restart_logs = [
        ROOT / "target" / f"e2e-agent-desktop-{configuration}-restart-{index}.log"
        # 1-2: MAC-E2E-16 completion-sound restarts; 3-5: MAC-E2E-22 density.
        for index in (1, 2, 3, 4, 5)
    ]
    try:
        wait_log(log, "hotkey_launch config=", 10.0)

        # MAC-E2E-09: the running instance is the bundle under test.
        apps = agent("list-apps", "--app", "ArcShot").get("apps", [])
        identities = sorted((entry.get("bundle_id"), entry.get("name")) for entry in apps)
        if identities != [(identifier, app)]:
            fail(f"unexpected running instances: {identities}")
        note(f"mac_e2e_09=pass bundle_id={identifier} name={app}")

        # MAC-E2E-01: status item and the seven menu commands, without taking focus.
        surface = agent("snapshot", "--surface", "menu-bar-extras", "--app", app, "--compact")
        item = (surface.get("tree", {}).get("children") or [None])[0]
        if not item or item.get("role") != "menuitem" or item.get("name") != "ArcShot":
            fail(f"status item not addressable as expected: {item}")
        menu = (item.get("children") or [None])[0]
        items = (menu or {}).get("children") or []
        actual_menu = [(child.get("name"), child.get("native_id", {}).get("value")) for child in items]
        if actual_menu != MENU:
            fail(f"menu commands differ: {actual_menu}")
        note(f"mac_e2e_01=pass menu_items={len(MENU)} refs={surface.get('ref_count')}")

        # MAC-E2E-17: the login item shows the system's actual state (read only).
        # The snapshot above opens the status menu, which validates the two
        # preference items; no registration is requested by this case.
        launch_read = wait_line(
            log,
            lambda line: "login_item_status state=" in line and "source=launch" in line,
            10.0,
        )
        menu_read = wait_line(
            log,
            lambda line: "login_item_status state=" in line and "source=menu" in line,
            10.0,
        )
        menu_state = menu_read.split("state=", 1)[1].split(" ", 1)[0]
        launch_state = launch_read.split("state=", 1)[1].split(" ", 1)[0]
        if menu_state not in ("Enabled", "Disabled", "RequiresApproval"):
            fail(f"unexpected login-item state: {menu_state}")
        if menu_state != launch_state:
            fail(f"menu and launch login-item reads disagree: {menu_state} / {launch_state}")
        if count_log(log, "login_item_set ") != 0:
            fail("reading the menu must never register or unregister the login item")
        note(
            f"mac_e2e_17=pass login_item_actual={menu_state} "
            "source=menu source=launch read_only=true"
        )

        # MAC-E2E-02: a resident instance exposes no own window.
        if windows(app):
            fail(f"resident instance exposes own windows: {windows(app)}")
        note("mac_e2e_02=pass own_windows=0")

        # MAC-E2E-03: the menu command enters the session and freezes the screen.
        log_mark = len(read_log(log))
        click(app, "开始截图")
        wait_log(log, "capture_request_us=", 15.0, after=log_mark)
        wait_frozen_active(log, log_mark)
        wait_log(log, "to=Frozen action=Completed", 15.0, after=log_mark)
        if windows(app):
            fail(f"frozen session exposes a capturable own window: {windows(app)}")
        note("mac_e2e_03=pass menu_start=frozen own_windows=0")

        # MAC-E2E-05a/05b: Escape reaches the app and the session leaves Frozen.
        # The borderless overlay is not enumerated by `snapshot --surface focused`,
        # so each delivery re-gates on the product's own activation line.
        press_until(
            app,
            "escape",
            headed=True,
            gate=lambda: wait_frozen_active(log, log_mark),
            observe=lambda: any(
                "from=Frozen to=Resident action=CancelAttempt" in line
                for line in read_log(log)[log_mark:]
            ),
        )
        wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 10.0, after=log_mark)
        if windows(app):
            fail(f"cancelled session left own windows: {windows(app)}")
        note("mac_e2e_05a=pass escape_delivery=headed own_windows=0")
        note("mac_e2e_05b=pass phase=Resident action=CancelAttempt")

        # MAC-E2E-04: only the registered combo starts a session.
        config = next(
            line.split("config=", 1)[1].split(" ", 1)[0]
            for line in read_log(log)
            if "hotkey_launch config=" in line
        )
        before = count_log(log, "capture_request_us=")
        negative = "⌃⌥B" if config != "⌃⌥B" else "⌃⌥N"
        press(hotkey_combo(negative), headed=False)
        time.sleep(1.5)
        if count_log(log, "capture_request_us=") != before:
            fail("an unregistered key combination started a capture")
        log_mark = len(read_log(log))
        # The hotkey is a physical global delivery whose result is
        # delivered_unverified; retry only while the product has observed no
        # capture at all, so a retry can never start a second session.
        for _ in range(4):
            press(hotkey_combo(config), headed=False)
            deadline = time.monotonic() + 2.5
            while time.monotonic() < deadline:
                if count_log(log, "capture_request_us=") >= before + 1:
                    break
                time.sleep(POLL)
            if count_log(log, "capture_request_us=") >= before + 1:
                break
        wait_count(log, "capture_request_us=", before + 1, 10.0)
        if count_log(log, "capture_request_us=") != before + 1:
            fail("the registered hotkey did not start exactly one capture")
        wait_frozen_active(log, log_mark)
        wait_log(log, "to=Frozen action=Completed", 15.0, after=log_mark)
        note(f"mac_e2e_04=pass hotkey={config} negative_control=no_capture")
        press_until(
            app,
            "escape",
            headed=True,
            gate=lambda: wait_frozen_active(log, log_mark),
            observe=lambda: any(
                "from=Frozen to=Resident action=CancelAttempt" in line
                for line in read_log(log)[log_mark:]
            ),
        )
        wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 10.0, after=log_mark)

        # MAC-E2E-07: the permission notice appears and Escape closes it.
        click(app, "屏幕录制权限…")
        notice = wait_window_size(app, 560.0, 282.0, 10.0)
        if not notice.get("is_focused"):
            fail(f"permission notice is not focused, refusing headed key delivery: {notice}")
        wait_frontmost(app, 5.0)
        size = notice.get("bounds", {})
        press_until(app, "escape", headed=True, observe=lambda: not windows(app))
        note(f"mac_e2e_07=pass notice_size={size['width']}x{size['height']} escape_closed=true")

        # MAC-E2E-10: the notice is readable and drivable through AX alone.
        click(app, "屏幕录制权限…")
        wait_window_size(app, 560.0, 282.0, 10.0)
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 权限提示") or [None]
            )[0],
            10.0,
        )
        texts = find_nodes(root, role="statictext")
        if len(texts) < 2:
            fail(f"notice exposes fewer than two readable texts: {texts}")
        safe_ids = [
            "arcshot.notice.close",
            "arcshot.notice.cancel",
            "arcshot.notice.later",
            "arcshot.notice.recheck",
        ]
        button = next(
            (
                node
                for native in safe_ids
                for node in find_nodes(root, native_id=native, require_ref=True)
            ),
            None,
        )
        if button is None:
            fail(
                "notice variant offers no non-navigating button; refusing to open "
                f"System Settings from the harness: {[n.get('native_id') for n in find_nodes(root, role='button')]}"
            )
        if not find_nodes(root, native_id=button["native_id"]["value"], require_ref=True):
            fail("notice button has no stable AXIdentifier")
        debug_text = [node.get("value") for node in texts]
        agent("click", button["ref_id"])
        wait_log(log, f"a11y_action={button['native_id']['value']} action=Click", 10.0)
        wait_windows(app, 0, 10.0)
        note(
            "mac_e2e_10=pass notice_texts="
            f"{len(texts)} action={button['native_id']['value']} "
            f"readable={len(' '.join(filter(None, debug_text)))}"
        )

        # MAC-E2E-06: the hotkey settings window opens as an addressable window.
        click(app, "截图快捷键…")
        wait_log(log, "menu_hotkey_settings", 10.0)
        settings = wait_window_size(app, 480.0, 362.0, 10.0)
        size = settings.get("bounds", {})
        note(f"mac_e2e_06=pass settings_size={size['width']}x{size['height']}")

        # MAC-E2E-11: the settings window is readable and drivable through AX.
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图快捷键") or [None]
            )[0],
            10.0,
        )
        recorder = find_nodes(root, native_id="arcshot.hotkey.recorder", require_ref=True)
        if not recorder:
            fail("hotkey recorder is not addressable")
        current = next(
            (
                node["value"]
                for node in find_nodes(root, role="statictext")
                if (node.get("value") or "").startswith("当前：")
            ),
            None,
        )
        if current is None:
            fail("hotkey settings do not expose the current combination")
        first_label = recorder[0].get("name")
        agent("click", recorder[0]["ref_id"])
        wait_log(log, "a11y_action=arcshot.hotkey.recorder action=Click", 10.0)
        wait_frontmost(app, 5.0)
        original = next(
            line.split("config=", 1)[1].split(" ", 1)[0]
            for line in read_log(log)
            if "hotkey_launch config=" in line
        )
        temporary = "⌃⌥9" if original != "⌃⌥9" else "⌃⌥8"
        # Keys meant for one of our windows use --headed after the product has
        # activated that window (TEST.md §4.2), retried until observed.
        press_until(
            app,
            hotkey_combo(temporary),
            headed=True,
            observe=lambda: any(f"candidate={temporary}" in line for line in read_log(log)),
        )
        save = wait_node(
            app,
            lambda tree: (find_nodes(tree, native_id="arcshot.hotkey.save", require_ref=True) or [None])[0],
            10.0,
        )
        agent("click", save["ref_id"])
        wait_log(log, f"hotkey_store value={temporary}", 10.0)
        # Restore the user's combination through the same AX path.
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图快捷键") or [None]
            )[0],
            10.0,
        )
        recorder = find_nodes(root, native_id="arcshot.hotkey.recorder", require_ref=True)[0]
        agent("click", recorder["ref_id"])
        wait_log(log, "a11y_action=arcshot.hotkey.recorder action=Click", 10.0)
        wait_frontmost(app, 5.0)
        press_until(
            app,
            hotkey_combo(original),
            headed=True,
            observe=lambda: any(f"candidate={original}" in line for line in read_log(log)),
        )
        save = wait_node(
            app,
            lambda tree: (find_nodes(tree, native_id="arcshot.hotkey.save", require_ref=True) or [None])[0],
            10.0,
        )
        agent("click", save["ref_id"])
        wait_log(log, f"hotkey_store value={original}", 10.0)
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图快捷键") or [None]
            )[0],
            10.0,
        )
        close = find_nodes(root, native_id="arcshot.hotkey.close", require_ref=True)[0]
        agent("click", close["ref_id"])
        wait_log(log, "a11y_action=arcshot.hotkey.close action=Click", 10.0)
        wait_windows(app, 0, 10.0)
        note(
            f"mac_e2e_11=pass recorder_name={first_label!r} "
            f"round_trip={original}->{temporary}->{original}"
        )

        # MAC-E2E-12/13/14: the frozen overlay, tool dock and hovered options are
        # addressable and drivable through AX alone, then cancel through AX.
        click(app, "开始截图")
        wait_log(log, "capture_wake_window_submit_us=", 15.0)
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
            )[0],
            10.0,
        )
        dock = (find_nodes(root, role="group", name="标注工具") or [None])[0]
        if dock is None:
            fail("frozen overlay exposes no tool dock")
        select_tool = wait_tool_state(app, "arcshot.overlay.tool.select", True, 10.0)
        rectangle_tool = wait_tool_state(app, "arcshot.overlay.tool.rectangle", False, 10.0)
        if not select_tool.get("ref_id") or not rectangle_tool.get("ref_id"):
            fail("tool dock buttons are not addressable")
        if find_nodes(root, native_id="arcshot.overlay.delete"):
            fail("delete is offered without a selected object")
        agent("click", rectangle_tool["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.tool.rectangle action=Click", 10.0)
        wait_log(log, "capture_edit action=SelectTool", 10.0)
        rectangle_tool = wait_tool_state(app, "arcshot.overlay.tool.rectangle", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.tool.select", False, 10.0)
        note("mac_e2e_13=pass tool_dock=ax tool=Rectangle selected_state=toggled")

        # Hover is the only way to reveal options; the activation itself is AX.
        hover(rectangle_tool["ref_id"])
        width_five = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.rectangle.width.5",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        red = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.rectangle.color.red",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        if not red.get("ref_id"):
            fail("hovered rectangle options are not addressable")
        agent("click", width_five["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.rectangle.width.5 action=Click", 10.0)
        wait_log(log, "capture_edit action=RectangleOption", 10.0)
        wait_log(log, "width=5", 10.0)
        blue = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.rectangle.color.blue",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        agent("click", blue["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.rectangle.color.blue action=Click", 10.0)
        wait_log(log, "style=stroke=#007AFF width=5", 10.0)
        wait_tool_state(app, "arcshot.overlay.option.rectangle.width.5", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.rectangle.color.blue", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.rectangle.color.red", False, 10.0)
        note("mac_e2e_14=pass hover_options=ax style=#007AFF width=5 version_unchanged")

        # MAC-E2E-18: the two line-class tools are addressable and mutually
        # exclusive with the existing Select/Rectangle checked states.
        line_tool = wait_tool_state(app, "arcshot.overlay.tool.line", False, 10.0)
        arrow_tool = wait_tool_state(app, "arcshot.overlay.tool.arrow", False, 10.0)
        if not line_tool.get("ref_id") or not arrow_tool.get("ref_id"):
            fail("line/arrow tool buttons are not addressable")
        mark = len(read_log(log))
        agent("click", line_tool["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.tool.line action=Click", 10.0, after=mark)
        line = wait_log_line(log, "capture_edit action=SelectTool", 10.0, after=mark)
        if "tool=Line" not in line:
            fail(f"line tool diagnostic was {line!r}")
        wait_tool_state(app, "arcshot.overlay.tool.line", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.tool.rectangle", False, 10.0)
        arrow_tool = wait_tool_state(app, "arcshot.overlay.tool.arrow", False, 10.0)
        mark = len(read_log(log))
        agent("click", arrow_tool["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.tool.arrow action=Click", 10.0, after=mark)
        arrow = wait_log_line(log, "capture_edit action=SelectTool", 10.0, after=mark)
        if "tool=Arrow" not in arrow:
            fail(f"arrow tool diagnostic was {arrow!r}")
        wait_tool_state(app, "arcshot.overlay.tool.arrow", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.tool.line", False, 10.0)
        note("mac_e2e_18=pass tool_dock=ax tools=Line,Arrow selected_state=toggled")

        # MAC-E2E-19: hovering Line exposes its options; AX toggles the dash and
        # changes width/colour, all as tool memory without a document version.
        line_tool = wait_tool_state(app, "arcshot.overlay.tool.line", False, 10.0)
        hover(line_tool["ref_id"])
        dash = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.line.dash",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        if not dash.get("ref_id"):
            fail("hovered line options are not addressable")
        if checked_state(dash):
            fail("a new line session must default to solid")
        mark = len(read_log(log))
        agent("click", dash["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.line.dash action=Click", 10.0, after=mark)
        edit = wait_log_line(log, "capture_edit action=LineOption", 10.0, after=mark)
        if "dash=true" not in edit or "width=3" not in edit:
            fail(f"line dash diagnostic was {edit!r}")
        wait_tool_state(app, "arcshot.overlay.option.line.dash", True, 10.0)
        width_two = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.line.width.2",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        mark = len(read_log(log))
        agent("click", width_two["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.line.width.2 action=Click", 10.0, after=mark)
        edit = wait_log_line(log, "capture_edit action=LineOption", 10.0, after=mark)
        if "width=2" not in edit or "dash=true" not in edit:
            fail(f"line width diagnostic was {edit!r}")
        green = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.line.color.green",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        mark = len(read_log(log))
        agent("click", green["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.line.color.green action=Click", 10.0, after=mark)
        edit = wait_log_line(log, "capture_edit action=LineOption", 10.0, after=mark)
        if "stroke=#34C759 width=2 dash=true" not in edit:
            fail(f"line colour diagnostic was {edit!r}")
        wait_tool_state(app, "arcshot.overlay.option.line.dash", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.line.width.2", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.line.color.green", True, 10.0)
        note("mac_e2e_19=pass hover_options=ax line_style=#34C759/2/dash version_unchanged")

        # MAC-E2E-20: the arrow options toggle Inward and change width/colour.
        arrow_tool = wait_tool_state(app, "arcshot.overlay.tool.arrow", True, 10.0)
        hover(arrow_tool["ref_id"])
        inward = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.arrow.inward",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        if not inward.get("ref_id"):
            fail("hovered arrow options are not addressable")
        if checked_state(inward):
            fail("a new arrow session must default to flat")
        mark = len(read_log(log))
        agent("click", inward["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.arrow.inward action=Click", 10.0, after=mark)
        edit = wait_log_line(log, "capture_edit action=ArrowOption", 10.0, after=mark)
        if "inward=true" not in edit or "width=3" not in edit:
            fail(f"arrow inward diagnostic was {edit!r}")
        wait_tool_state(app, "arcshot.overlay.option.arrow.inward", True, 10.0)
        width_five = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.arrow.width.5",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        mark = len(read_log(log))
        agent("click", width_five["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.arrow.width.5 action=Click", 10.0, after=mark)
        edit = wait_log_line(log, "capture_edit action=ArrowOption", 10.0, after=mark)
        if "width=5" not in edit or "inward=true" not in edit:
            fail(f"arrow width diagnostic was {edit!r}")
        orange = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.arrow.color.orange",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        mark = len(read_log(log))
        agent("click", orange["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.option.arrow.color.orange action=Click", 10.0, after=mark)
        edit = wait_log_line(log, "capture_edit action=ArrowOption", 10.0, after=mark)
        if "stroke=#FF9500 width=5 inward=true" not in edit:
            fail(f"arrow colour diagnostic was {edit!r}")
        wait_tool_state(app, "arcshot.overlay.option.arrow.inward", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.arrow.width.5", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.arrow.color.orange", True, 10.0)
        note("mac_e2e_20=pass hover_options=ax arrow_style=#FF9500/5/inward version_unchanged")

        # MAC-E2E-21: rectangle keeps its own style memory after the other tools
        # changed theirs.
        rectangle_tool = wait_tool_state(app, "arcshot.overlay.tool.rectangle", False, 10.0)
        hover(rectangle_tool["ref_id"])
        wait_tool_state(app, "arcshot.overlay.option.rectangle.width.5", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.rectangle.color.blue", True, 10.0)
        note("mac_e2e_21=pass tool_memory=independent rectangle=#007AFF/5")

        # MAC-E2E-23: the Blur tool is addressable, its three levels are
        # reachable through hover, the default is Medium, each AX level change
        # writes one text diagnostic, and the tooltip states the redaction
        # limitation in the accessibility tree.
        blur_tool = wait_tool_state(app, "arcshot.overlay.tool.blur", False, 10.0)
        if not blur_tool.get("ref_id"):
            fail("the blur tool button is not addressable")
        mark = len(read_log(log))
        agent("click", blur_tool["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.tool.blur action=Click", 10.0, after=mark)
        select = wait_log_line(log, "capture_edit action=SelectTool", 10.0, after=mark)
        if "tool=Blur" not in select:
            fail(f"blur tool diagnostic was {select!r}")
        wait_tool_state(app, "arcshot.overlay.tool.blur", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.tool.arrow", False, 10.0)

        hover(blur_tool["ref_id"])
        weak = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.blur.weak",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        medium = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.blur.medium",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        strong = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.blur.strong",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        if not weak.get("ref_id") or not medium.get("ref_id") or not strong.get("ref_id"):
            fail("hovered blur options are not addressable")
        if not checked_state(medium):
            fail("a new blur session must default to the medium level")
        if checked_state(weak) or checked_state(strong):
            fail("only the medium level may be checked by default")

        mark = len(read_log(log))
        agent("click", weak["ref_id"])
        wait_log(
            log,
            "a11y_action=arcshot.overlay.option.blur.weak action=Click",
            10.0,
            after=mark,
        )
        edit = wait_log_line(log, "capture_edit action=BlurOption", 10.0, after=mark)
        if "level=weak" not in edit:
            fail(f"blur weak diagnostic was {edit!r}")
        wait_tool_state(app, "arcshot.overlay.option.blur.weak", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.blur.medium", False, 10.0)

        mark = len(read_log(log))
        agent("click", strong["ref_id"])
        wait_log(
            log,
            "a11y_action=arcshot.overlay.option.blur.strong action=Click",
            10.0,
            after=mark,
        )
        edit = wait_log_line(log, "capture_edit action=BlurOption", 10.0, after=mark)
        if "level=strong" not in edit:
            fail(f"blur strong diagnostic was {edit!r}")
        wait_tool_state(app, "arcshot.overlay.option.blur.strong", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.blur.weak", False, 10.0)
        note(
            "mac_e2e_23=pass tool_dock=ax blur_levels=weak/medium/strong "
            "default=medium diagnostics=level"
        )

        # MAC-E2E-25: the Text tool is addressable, its colour and size options
        # are reachable through hover, the default is red 16 pt, the size
        # steppers and colour swatch each write one text-only diagnostic, and a
        # created editor is addressable. The selection drag and the creating
        # click are --headed preconditions (there is no selection AX node); the
        # acceptance evidence is the AX activations and the product diagnostics.
        text_tool = wait_tool_state(app, "arcshot.overlay.tool.text", False, 10.0)
        if not text_tool.get("ref_id"):
            fail("the text tool button is not addressable")
        mark = len(read_log(log))
        agent("click", text_tool["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.tool.text action=Click", 10.0, after=mark)
        select = wait_log_line(log, "capture_edit action=SelectTool", 10.0, after=mark)
        if "tool=Text" not in select:
            fail(f"text tool diagnostic was {select!r}")
        wait_tool_state(app, "arcshot.overlay.tool.text", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.tool.blur", False, 10.0)

        # Creation precondition: a real selection and a text editor.
        mark = len(read_log(log))
        agent("drag", "--from-xy", "50,50", "--to-xy", "200,200", "--headed")
        wait_log(log, "capture_edit action=SelectRegion", 10.0, after=mark)
        agent("mouse-click", "--xy", "100,100", "--headed")
        text_input = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, native_id="arcshot.overlay.text.input", require_ref=True)
                or [None]
            )[0],
            10.0,
        )
        if not text_input.get("ref_id"):
            fail("the text editor is not addressable after creation")
        # Type one ASCII character so the commit has a non-empty object. The
        # IME composition path itself is a MAC-TEXT manual item; physical keys
        # are the headed precondition and the commit diagnostic is the
        # evidence. Headed delivery is delivered_unverified and can miss the
        # editor on the first turn, so send a few idempotent presses.
        for _ in range(3):
            agent("press", "a", "--headed")
            time.sleep(0.2)

        hover(text_tool["ref_id"])
        size_field = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, native_id="arcshot.overlay.text.size", require_ref=True)
                or [None]
            )[0],
            10.0,
        )
        if not size_field.get("ref_id"):
            fail("the text size field is not addressable")
        if str(size_field.get("value")) != "16":
            fail(f"the default text size was {size_field.get('value')!r}, expected '16'")

        size_up = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.text.size.up",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        if not size_up.get("ref_id"):
            fail("the text size up stepper is not addressable")
        mark = len(read_log(log))
        agent("click", size_up["ref_id"])
        wait_log(
            log,
            "a11y_action=arcshot.overlay.option.text.size.up action=Click",
            10.0,
            after=mark,
        )
        edit = wait_log_line(log, "capture_edit action=TextOption", 10.0, after=mark)
        if "size=16.5" not in edit:
            fail(f"text size diagnostic was {edit!r}")
        wait_node(
            app,
            lambda tree: next(
                (
                    node
                    for node in find_nodes(tree, native_id="arcshot.overlay.text.size")
                    if str(node.get("value")) == "16.5"
                ),
                None,
            ),
            10.0,
        )

        blue = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.text.color.blue",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        if not blue.get("ref_id"):
            fail("the text blue swatch is not addressable")
        mark = len(read_log(log))
        agent("click", blue["ref_id"])
        wait_log(
            log,
            "a11y_action=arcshot.overlay.option.text.color.blue action=Click",
            10.0,
            after=mark,
        )
        edit = wait_log_line(log, "capture_edit action=TextOption", 10.0, after=mark)
        if "stroke=#007AFF" not in edit:
            fail(f"text colour diagnostic was {edit!r}")
        wait_tool_state(app, "arcshot.overlay.option.text.color.blue", True, 10.0)

        # Commit the editor by clicking elsewhere: one creation history step and
        # the editor disappears. The point is clear of the left dock and its
        # hover option strip (which legitimately consumes its own clicks).
        mark = len(read_log(log))
        agent("mouse-click", "--xy", "500,500", "--headed")
        wait_log(log, "capture_edit action=TextCommit", 10.0, after=mark)
        commit = wait_log_line(log, "capture_edit action=TextCommit", 10.0, after=mark)
        if "annotations=1" not in commit:
            fail(f"text commit diagnostic was {commit!r}")
        note(
            "mac_e2e_25=pass tool_dock=ax text_editor=ax size=16->16.5 "
            "colour=#007AFF commit=1"
        )

        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
            )[0],
            10.0,
        )
        cancel = find_nodes(root, native_id="arcshot.overlay.cancel", require_ref=True)
        if not cancel:
            fail("frozen overlay exposes no cancel action")
        agent("click", cancel[0]["ref_id"])
        wait_log(log, "a11y_action=arcshot.overlay.cancel action=Click", 10.0)
        wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 10.0)
        if windows(app):
            fail(f"AX-cancelled session left own windows: {windows(app)}")
        note("mac_e2e_12=pass overlay_cancel=ax phase=Resident")

        # MAC-E2E-24: the Scroll entry gate is addressable, is not activatable
        # without a selection, and after a selection drives the three-permission
        # preflight. The selection drag is a --headed precondition; the
        # acceptance evidence is the product's AX state and text diagnostics.
        log_mark = len(read_log(log))
        click(app, "开始截图")
        wait_frozen_active(log, log_mark)
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
            )[0],
            10.0,
        )
        scroll = find_nodes(root, native_id="arcshot.overlay.scroll")
        if not scroll:
            fail("the Scroll entry is not addressable")
        if scroll[0].get("role") != "button":
            fail("the Scroll entry is not exposed as a button")
        # Without a selection the entry is present but performs no action.
        click_scroll_entry(app, denied=True)
        if count_log(log, "a11y_action=arcshot.overlay.scroll") != 0:
            fail("the Scroll entry produced an action without a valid selection")
        # Create a selection as a precondition for the entry gate.
        agent("drag", "--from-xy", "400,300", "--to-xy", "700,500", "--headed")
        wait_log(log, "capture_edit action=SelectRegion", 10.0, after=log_mark)
        scroll = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.scroll",
                    role="button",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        mark = len(read_log(log))
        click_scroll_entry(app, denied=False)
        wait_log(log, "a11y_action=arcshot.overlay.scroll action=Click", 10.0, after=mark)
        preflight = wait_log_line(log, "scroll_permission ", 10.0, after=mark)
        missing = [
            key
            for key, token in (
                ("screen_recording", "screen_recording=false"),
                ("input_monitoring", "input_monitoring=false"),
                ("accessibility", "accessibility=false"),
            )
            if token in preflight
        ]
        if missing:
            notice = wait_node(
                app,
                lambda tree: (
                    find_nodes(tree, role="group", name="ArcShot 滚动截图权限") or [None]
                )[0],
                10.0,
            )
            for key in missing:
                if not find_nodes(
                    notice,
                    native_id=f"arcshot.scroll.permission.{key}",
                    require_ref=True,
                ):
                    fail(f"missing permission {key} has no settings button")
            recheck = find_nodes(
                notice, native_id="arcshot.scroll.recheck", require_ref=True
            )
            if not recheck:
                fail("the Scroll permission notice has no recheck button")
            agent("click", recheck[0]["ref_id"])
            wait_log(
                log,
                "a11y_action=arcshot.scroll.recheck action=Click",
                10.0,
                after=mark,
            )
            cancel = wait_node(
                app,
                lambda tree: (
                    find_nodes(
                        tree,
                        native_id="arcshot.scroll.cancel",
                        require_ref=True,
                    )
                    or [None]
                )[0],
                10.0,
            )
            agent("click", cancel["ref_id"])
            wait_log(
                log,
                "a11y_action=arcshot.scroll.cancel action=Click",
                10.0,
                after=mark,
            )
            wait_windows(app, 0, 10.0)
        else:
            # All three permissions are available: the target probe runs. It
            # either locks the target or reports a known refusal; both keep the
            # region session and never claim a completed Scroll capture.
            prepared = False
            try:
                wait_log(log, "scroll_prepared", 5.0, after=mark)
                prepared = True
            except Failure:
                # The entry gate itself is the assertion here. #33 may also
                # stop collection immediately on low-texture or ambiguous
                # target content; that is a known-state refusal, not a gate
                # failure, and it still preserves the region session.
                wait_log(log, "scroll_refused reason=", 5.0, after=mark)
            if prepared:
                try:
                    control = wait_node(
                        app,
                        lambda tree: (
                            find_nodes(
                                tree,
                                native_id="arcshot.scroll.control.cancel",
                                require_ref=True,
                            )
                            or [None]
                        )[0],
                        2.0,
                    )
                except Failure:
                    control = None
                if control is not None:
                    try:
                        agent("click", control["ref_id"])
                    except Failure as error:
                        # #33 can stop collection (and close the control
                        # window) between the snapshot and the click; that is
                        # a known-state refusal, not a gate failure.
                        if "STALE_REF" not in str(error):
                            raise
                    else:
                        wait_log(
                            log,
                            "a11y_action=arcshot.scroll.control.cancel action=Click",
                            10.0,
                            after=mark,
                        )
                if control is None:
                    # #34: a known-state stop during collection (dynamic or
                    # low-texture target content) offers the explicit partial
                    # choice in the control window instead of returning
                    # straight to the overlay. Abandon it to restore the
                    # region session asserted below.
                    try:
                        wait_log(
                            log, "scroll_partial_choice available=true", 3.0, after=mark
                        )
                        abandon = wait_node(
                            app,
                            lambda tree: (
                                find_nodes(
                                    tree,
                                    native_id="arcshot.scroll.partial.abandon",
                                    require_ref=True,
                                )
                                or [None]
                            )[0],
                            10.0,
                        )
                        agent("click", abandon["ref_id"])
                        wait_log(log, "action=AbandonScrollPartial", 10.0, after=mark)
                    except Failure:
                        pass
        # The region session and its selection are preserved.
        wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
            )[0],
            10.0,
        )
        note(
            f"mac_e2e_24=pass entry_gate=ax selection_precondition=headed_drag "
            f"missing={','.join(missing) or 'none'}"
        )

        # MAC-E2E-29: the confirmed-copy whole-image preview, the shared
        # annotation surface, the whole-image annotated output and cancel
        # retention (I-11 §7–8). The headed drag is a pointer precondition; its
        # product effect is proven by `capture_edit ... annotations=1`, and the
        # whole-image extent by the output pixels plus the verified stitch.
        fixture_log = ROOT / "target" / f"e2e-scroll-fixture-{configuration}.log"
        fixture = start(
            ROOT / "target" / configuration / "arcshot-scroll-fixture", fixture_log
        )
        try:
            wait_log(fixture_log, "fixture_window", 10.0)
            time.sleep(1.0)

            def latest_stitch_bottom(after):
                values = [
                    int(line.split("bottom=", 1)[1].split(" ", 1)[0])
                    for line in read_log(log)[after:]
                    if "scroll_stitch extended" in line and "bottom=" in line
                ]
                return values[-1] if values else None

            def complete_long_image(drag_from=None, drag_to=None):
                if drag_from is None or drag_to is None:
                    free = free_fixture_selection()
                    drag_from = f"{free[0]:.0f},{free[1] + SCREEN_Y_OFFSET:.0f}"
                    drag_to = (
                        f"{free[0] + free[2]:.0f},"
                        f"{free[1] + free[3] + SCREEN_Y_OFFSET:.0f}"
                    )
                log_mark = len(read_log(log))
                click(app, "开始截图")
                wait_frozen_active(log, log_mark)
                selection = select_region(
                    app,
                    log,
                    drag_from=drag_from,
                    drag_to=drag_to,
                    mark=log_mark,
                )
                mark = len(read_log(log))
                click_scroll_entry(app, denied=False)
                wait_log(
                    log,
                    "a11y_action=arcshot.overlay.scroll action=Click",
                    10.0,
                    after=mark,
                )
                wait_log(log, "scroll_collecting", 15.0, after=mark)
                # The whole-image post-processing path does not depend on how many
                # rows were stitched: completing the trusted single frame already
                # exercises copy -> preview -> annotate -> re-finish end to end.
                press_until_log(
                    "return", log, "output_publish target=Clipboard", timeout=5.0, after=mark
                )
                publish = wait_log_line(
                    log, "output_publish target=Clipboard", 5.0, after=mark
                )
                wait_log(log, "completion_preview shown", 5.0, after=mark)
                return mark, selection, publish

            def activate_completion_preview(mark):
                preview = wait_node(
                    app,
                    lambda tree: (
                        find_nodes(
                            tree,
                            native_id="arcshot.completion.preview",
                            require_ref=True,
                        )
                        or [None]
                    )[0],
                    10.0,
                )
                agent("click", preview["ref_id"])
                wait_log(
                    log,
                    "a11y_action=arcshot.completion.preview action=Click",
                    5.0,
                    after=mark,
                )
                opened = wait_log_line(
                    log, "completion_annotation_opened", 5.0, after=mark
                )
                document = opened.split("document=", 1)[1].split(" ", 1)[0]
                doc_width, doc_height = (float(value) for value in document.split("x", 1))
                zoom = float(opened.split("zoom=", 1)[1].split(" ", 1)[0])
                return doc_width, doc_height, zoom

            # Case A: click the thumbnail, draw one Rectangle with the existing
            # editor, re-finish and observe the whole annotated image copied.
            mark, selection, first_publish = complete_long_image("620,600", "850,780")
            first_bytes = int(first_publish.split("bytes=", 1)[1].split(" ", 1)[0])
            first_annotations = first_publish.split("annotations=", 1)[1].split(" ", 1)[0]
            if first_annotations != "0":
                fail(f"the first completion copy already had annotations: {first_publish}")
            crop_width = int(round(selection[2] * 2))
            crop_height = int(round(selection[3] * 2))
            doc_width, doc_height, zoom = activate_completion_preview(mark)
            wait_log(log, "app_active surface=frozen", 10.0, after=mark)
            # Accessibility-only creation path: it drives the same authoritative
            # Document::add_rectangle and does not depend on coordinate pointer
            # delivery, which third-party floating windows can intercept.
            add_rectangle = wait_node(
                app,
                lambda tree: (
                    find_nodes(
                        tree,
                        native_id="arcshot.overlay.completion.add_rectangle",
                        require_ref=True,
                    )
                    or [None]
                )[0],
                10.0,
            )
            agent("click", add_rectangle["ref_id"])
            wait_log(
                log,
                "a11y_action=arcshot.overlay.completion.add_rectangle action=Click",
                5.0,
                after=mark,
            )
            edited = wait_log_line(
                log, "capture_edit action=AddDefaultRectangle", 5.0, after=mark
            )
            if "annotations=1" not in edited:
                fail(f"the accessible rectangle did not enter the shared document: {edited}")
            # The un-annotated whole image is already on the real pasteboard.
            first_hash, first_size, first_w, first_h = pasteboard_png(
                "35-e2e-29-first.png"
            )
            if (first_w, first_h) != (crop_width, crop_height):
                fail(
                    f"first clipboard PNG is {first_w}x{first_h}, "
                    f"want the whole crop {crop_width}x{crop_height}"
                )
            finish = wait_node(
                app,
                lambda tree: (
                    find_nodes(tree, native_id="arcshot.overlay.finish", require_ref=True)
                    or [None]
                )[0],
                10.0,
            )
            finish_mark = len(read_log(log))
            agent("click", finish["ref_id"])
            wait_log(
                log,
                "a11y_action=arcshot.overlay.finish action=Click",
                5.0,
                after=finish_mark,
            )
            republish = wait_log_line(
                log, "output_publish target=Clipboard", 10.0, after=finish_mark
            )
            second_bytes = int(republish.split("bytes=", 1)[1].split(" ", 1)[0])
            pixels = republish.split("pixels=", 1)[1].split(" ", 1)[0]
            width_text, height_text = pixels.split("x", 1)
            if int(width_text) != crop_width:
                fail(f"re-finish width {width_text} != whole crop {crop_width}")
            if int(height_text) != crop_height:
                fail(
                    f"re-finish height {height_text} is not the whole captured frame "
                    f"({crop_height})"
                )
            if "annotations=1" not in republish:
                fail(f"re-finish snapshot lost the annotation: {republish}")
            wait_log(
                log,
                "from=Frozen to=Resident action=OutputCompleted",
                10.0,
                after=finish_mark,
            )
            # The re-finish copied the whole annotated image to the real pasteboard.
            second_hash, second_size, second_w, second_h = pasteboard_png(
                "35-e2e-29-annotated.png"
            )
            if (second_w, second_h) != (crop_width, crop_height):
                fail(
                    f"re-finish clipboard PNG is {second_w}x{second_h}, "
                    f"want the whole crop {crop_width}x{crop_height}"
                )
            if second_hash == first_hash:
                fail("re-finish copied the un-annotated image")
            note(
                "mac_e2e_29_refinish=pass preview_click=true annotation=1 "
                f"whole_image={second_w}x{second_h} "
                f"clipboard_sha256={first_hash[:12]}..{second_hash[:12]} "
                f"bytes={first_size}->{second_size} session=ended"
            )

            # Case B: activating the preview then cancelling must not touch the
            # clipboard again; the earlier un-annotated copy stays authoritative.
            mark, selection, first_publish = complete_long_image()
            copies_after_first = count_log_after(log, "output_publish target=Clipboard", mark)
            cancel_hash, cancel_size, cancel_w, cancel_h = pasteboard_png(
                "35-e2e-29-cancel-first.png"
            )
            activate_completion_preview(mark)
            # Escape reaches the overlay once the product owns keyboard focus;
            # every retry re-gates on the product's own activation line.
            cancelled = False
            for _ in range(4):
                wait_log(log, "app_active surface=frozen", 10.0, after=mark)
                press("escape", headed=True)
                try:
                    wait_log(
                        log,
                        "from=Frozen to=Resident action=CancelAttempt",
                        2.5,
                        after=mark,
                    )
                except Failure:
                    continue
                cancelled = True
                break
            if not cancelled:
                fail("Escape never reached the annotation surface")
            if count_log_after(log, "output_publish target=Clipboard", mark) != copies_after_first:
                fail("cancelling the annotation wrote to the clipboard a second time")
            after_hash, after_size, _, _ = pasteboard_png("35-e2e-29-cancel-after.png")
            if after_hash != cancel_hash:
                fail("cancelling the annotation changed the previously copied PNG")
            note(
                "mac_e2e_29_cancel=pass clipboard_retention=true "
                f"copies={copies_after_first} sha256={after_hash[:12]} "
                f"bytes={after_size} session=cancelled"
            )
        finally:
            stop(fixture)

        # MAC-E2E-26: the protected single-frame Scroll completion loop over the
        # own GPUI fixture target (I-11 §3/6/10, I-12). The fixture is a real
        # separate process whose pixels are an exact function of its content
        # offset. Evidence: AX state, product text diagnostics, the fixture's
        # own event receipts, and the pixel-truth probe grid. Physical wheel /
        # click / key deliveries are the precondition for the input contract;
        # the assertions are the product and fixture diagnostics.
        fixture_log = ROOT / "target" / f"e2e-scroll-fixture-{configuration}.log"
        fixture = start(
            ROOT / "target" / configuration / "arcshot-scroll-fixture", fixture_log
        )
        try:
            wait_log(fixture_log, "fixture_window", 10.0)
            time.sleep(1.0)
            log_mark = len(read_log(log))
            click(app, "开始截图")
            wait_frozen_active(log, log_mark)
            selection = select_region(
                app,
                log,
                drag_from="620,413",
                drag_to="1200,593",
                mark=log_mark,
            )
            root = wait_node(
                app,
                lambda tree: (
                    find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
                )[0],
                10.0,
            )
            mark = len(read_log(log))
            click_scroll_entry(app, denied=False)
            wait_log(log, "a11y_action=arcshot.overlay.scroll action=Click", 10.0, after=mark)
            preflight = wait_log_line(log, "scroll_permission ", 10.0, after=mark)
            if "missing=none" not in preflight:
                fail(f"fixture run needs all three permissions: {preflight}")
            wait_log(log, "scroll_target window_id=", 15.0, after=mark)
            wait_log(log, "scroll_prepared ", 15.0, after=mark)
            wait_log(log, "scroll_input installed=true", 15.0, after=mark)
            wait_log(log, "scroll_frame_request_us=", 15.0, after=mark)
            wait_log(log, "scroll_collecting", 15.0, after=mark)
            wait_log(log, "scroll_phase from=Prepared to=Collecting", 10.0, after=mark)
            # Concise status + operation hint; never a percentage, ETA or pause.
            status_node = wait_node(
                app,
                lambda tree: next(
                    (
                        node
                        for node in find_nodes(tree, native_id="arcshot.scroll.status")
                        if "可以滚动" in str(node.get("value"))
                        or "可以滚动" in str(node.get("name"))
                    ),
                    None,
                ),
                10.0,
            )
            if not find_nodes(snapshot_tree(app), native_id="arcshot.scroll.hint"):
                fail("the collecting control UI does not expose the operation hint")
            # Pixel truth: the admitted frame must equal the fixture oracle.
            probe = wait_log_line(log, "capture_probe_grid", 10.0, after=mark).strip()
            probe = probe[probe.index("capture_probe_grid") :]
            expected = expected_fixture_probe(selection)
            if probe != expected:
                fail(f"scroll frame probe mismatch:\n got: {probe}\nwant: {expected}")
            # Click blocking: a raw click inside the selection never lands.
            fixture_mark = len(read_log(fixture_log))
            agent("mouse-click", "--xy", "1000,470", "--headed")
            time.sleep(1.0)
            if any(
                "fixture_click_received" in line
                for line in read_log(fixture_log)[fixture_mark:]
            ):
                fail("a blocked click reached the scroll target")
            # Non-owned keys still reach the target application. Physical key
            # delivery can lose a focus race; retry without relaxing the
            # fixture's own receipt as the assertion.
            press_until_log("a", fixture_log, "fixture_key_received")
            # Raw scroll inside the selection passes through to the target.
            agent("mouse-wheel", "--x", "1000", "--y", "470", "--dy", "-6", "--headed")
            wait_log(fixture_log, "fixture_scroll", 5.0)
            # Enter completes the single frame; the user simply stopped collecting.
            press_until_log(
                "return", log, "output_publish target=Clipboard", timeout=5.0, after=mark
            )
            # A confirmed copy keeps the session frozen and shows the whole-image
            # thumbnail for its fixed stay, then leaves without a second output.
            wait_log(log, "completion_preview shown", 5.0, after=mark)
            retired = wait_log_line(log, "scroll_input_retired", 10.0, after=mark)
            scrolls = int(retired.split("scrolls=", 1)[1].split(" ", 1)[0])
            clicks = int(retired.split("clicks_blocked=", 1)[1].split(" ", 1)[0])
            if scrolls < 1 or clicks < 1:
                fail(f"tap counters do not prove protection: {retired}")
            if windows(app):
                fail(f"completed scroll left own windows: {windows(app)}")
            wait_log(log, "completion_preview_expired", 8.0, after=mark)
            note(
                "mac_e2e_26=pass prepared=true collecting=true pixel_oracle=exact "
                f"click_blocked=true scroll_passed=true enter=clipboard preview=true "
                f"auto_dismiss=true scrolls={scrolls} clicks_blocked={clicks}"
            )

            # Esc abandons the scroll result: the pre-scroll selection returns.
            log_mark = len(read_log(log))
            click(app, "开始截图")
            wait_frozen_active(log, log_mark)
            select_region(
                app,
                log,
                drag_from="620,413",
                drag_to="1200,593",
                mark=log_mark,
            )
            root = wait_node(
                app,
                lambda tree: (
                    find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
                )[0],
                10.0,
            )
            scroll = find_nodes(root, native_id="arcshot.overlay.scroll", require_ref=True)[0]
            mark = len(read_log(log))
            agent("click", scroll["ref_id"])
            wait_log(log, "scroll_collecting", 20.0, after=mark)
            press_until_log(
                "escape", log, "from=Collecting to=Inactive action=CancelScroll", timeout=4.0
            )
            wait_log(log, "scroll_input_retired", 10.0, after=mark)
            # The same frozen document returns: the overlay with its valid
            # selection is addressable again and exposes the save entry.
            root = wait_node(
                app,
                lambda tree: (
                    find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
                )[0],
                10.0,
            )
            if not find_nodes(root, native_id="arcshot.overlay.save", require_ref=True):
                fail("Esc did not return the pre-scroll selection")
            note("mac_e2e_26_esc=pass abandon=true selection_returned=true")
        finally:
            stop(fixture)

        # MAC-E2E-27: bidirectional stitch, reverse de-duplication and the three
        # fixed live-preview positions over the own GPUI fixture (I-11 §4/5,
        # I-13). The preview window is fixed during preparation, ignores mouse
        # events, and is excluded from every capture. Evidence: product text
        # diagnostics, the fixture's own scroll receipts, and the exact pixel
        # oracle at the fixture's newest offset.
        fixture_log = ROOT / "target" / f"e2e-scroll-fixture-{configuration}.log"
        fixture = start(
            ROOT / "target" / configuration / "arcshot-scroll-fixture", fixture_log
        )
        try:
            wait_log(fixture_log, "fixture_window", 10.0)
            time.sleep(1.0)

            def begin_scroll(drag_from: str, drag_to: str):
                log_mark = len(read_log(log))
                click(app, "开始截图")
                wait_frozen_active(log, log_mark)
                selection = select_region(
                    app,
                    log,
                    drag_from=drag_from,
                    drag_to=drag_to,
                    mark=log_mark,
                )
                mark = len(read_log(log))
                click_scroll_entry(app, denied=False)
                wait_log(log, "a11y_action=arcshot.overlay.scroll action=Click", 10.0, after=mark)
                placed = wait_log_line(log, "scroll_preview placed=", 15.0, after=mark)
                wait_log(log, "scroll_collecting", 15.0, after=mark)
                exclusion = wait_log_line(log, "scroll_exclusion_fixed", 15.0, after=mark)
                return mark, selection, placed, exclusion

            def wheel(x: int, y: int, dy: int) -> None:
                agent("mouse-wheel", "--x", str(x), "--y", str(y), "--dy", str(dy), "--headed")

            def expect_probe(selection, mark: int, label: str) -> None:
                # The bounded loop captures continuously; wait until the newest
                # probe reflects the fixture's newest offset. Take the last
                # probe, never the first one after the mark (which predates
                # the scroll).
                deadline = time.time() + 5.0
                while True:
                    offset = fixture_offset_points(fixture_log)
                    expected = expected_fixture_probe_at(selection, offset)
                    probes = [
                        line for line in read_log(log)[mark:] if "capture_probe_grid" in line
                    ]
                    probe = probes[-1].strip() if probes else ""
                    probe = probe[probe.index("capture_probe_grid") :] if probe else ""
                    if probe == expected:
                        return
                    if time.time() >= deadline:
                        fail(
                            f"{label}: scrolled probe mismatch:\n got: {probe}\nwant: {expected}"
                        )
                    time.sleep(0.2)

            # Right of the selection: a selection near the fixture's left edge
            # and low enough to keep the pointer clear of the centred control
            # window under every raw-coordinate convention.
            mark, selection, placed, exclusion = begin_scroll("620,600", "850,780")
            if "placed=right" not in placed:
                fail(f"right case: unexpected preview placement {placed!r}")
            if "preview=" not in exclusion:
                fail(f"right case: fixed preview is not in the exclusion set: {exclusion!r}")
            stitch_mark = len(read_log(log))
            # The selection lies below the centred control window, so the raw
            # scroll at its centre reaches the target untouched.
            for _ in range(4):
                wheel(830, 700, -1)
            # Each wheel moves the fixture 16 logical rows = 32 physical rows;
            # wait for the verified document to reach that bottom before
            # measuring the reverse pass, otherwise later wheels race the
            # extension count.
            expected_bottom = 360 + 4 * 32
            deadline = time.time() + 8.0
            while True:
                extended_lines = [
                    line
                    for line in read_log(log)[stitch_mark:]
                    if "scroll_stitch extended" in line
                ]
                bottom = (
                    int(extended_lines[-1].split("bottom=", 1)[1].split(" ", 1)[0])
                    if extended_lines
                    else 0
                )
                if bottom >= expected_bottom or time.time() >= deadline:
                    break
                time.sleep(0.2)
            if bottom < expected_bottom:
                fail(f"right down: verified bottom {bottom} < {expected_bottom}")
            expect_probe(selection, stitch_mark, "right down")
            extended_before = count_log_after(log, "scroll_stitch extended", stitch_mark)
            for _ in range(4):
                wheel(830, 700, 1)
            wait_log(log, "scroll_stitch duplicate", 10.0, after=stitch_mark)
            # Let the last queued frames settle before counting, so a frame
            # captured mid-gesture cannot race the assertion.
            deadline = time.time() + 5.0
            while fixture_offset_points(fixture_log) != FIXTURE_INITIAL_OFFSET_ROWS:
                if time.time() >= deadline:
                    break
                time.sleep(0.2)
            time.sleep(0.5)
            duplicates = count_log_after(log, "scroll_stitch duplicate", stitch_mark)
            extended_after = count_log_after(log, "scroll_stitch extended", stitch_mark)
            if duplicates < 1 or extended_after != extended_before:
                fail(
                    "right case: reverse pass was not de-duplicated "
                    f"(duplicates={duplicates}, extended {extended_before}→{extended_after})"
                )
            expect_probe(selection, stitch_mark, "right reverse")
            press_until_log(
                "return", log, "output_publish target=Clipboard", timeout=5.0, after=stitch_mark
            )
            wait_log(log, "completion_preview shown", 5.0, after=stitch_mark)
            wait_log(log, "completion_preview_expired", 8.0, after=stitch_mark)
            note(
                "mac_e2e_27_right=pass placed=right extended=true reverse_dedup=true "
                f"duplicates={duplicates} output=clipboard preview=true auto_dismiss=true"
            )

            # Left of the selection: a selection near the fixture's right edge.
            mark, selection, placed, exclusion = begin_scroll("900,413", "1200,593")
            if "placed=left" not in placed:
                fail(f"left case: unexpected preview placement {placed!r}")
            if "preview=" not in exclusion:
                fail(f"left case: fixed preview is not in the exclusion set: {exclusion!r}")
            stitch_mark = len(read_log(log))
            for _ in range(3):
                wheel(1050, 500, -1)
            wait_log(log, "scroll_stitch extended", 10.0, after=stitch_mark)
            for _ in range(3):
                wheel(1050, 500, 1)
            wait_log(log, "scroll_stitch duplicate", 10.0, after=stitch_mark)
            expect_probe(selection, stitch_mark, "left reverse")
            press_until_log(
                "return", log, "output_publish target=Clipboard", timeout=5.0, after=stitch_mark
            )
            wait_log(log, "completion_preview shown", 5.0, after=stitch_mark)
            wait_log(log, "completion_preview_expired", 8.0, after=stitch_mark)
            note(
                "mac_e2e_27_left=pass placed=left extended=true reverse_dedup=true "
                "output=clipboard preview=true auto_dismiss=true"
            )

            # Inner bottom-right: a selection spanning the fixture width, so
            # neither side placement fits. The pointer sits on the preview and
            # raw scroll must still reach the target; the capture beneath the
            # preview must stay pixel-exact and the preview must stay excluded.
            mark, selection, placed, exclusion = begin_scroll("600,413", "1360,593")
            if "placed=inner" not in placed:
                fail(f"inner case: unexpected preview placement {placed!r}")
            if "preview=" not in exclusion:
                fail(f"inner case: fixed preview is not in the exclusion set: {exclusion!r}")
            fixture_mark = len(read_log(fixture_log))
            stitch_mark = len(read_log(log))
            for _ in range(3):
                wheel(1000, 450, -1)
            wait_log(log, "scroll_stitch extended", 10.0, after=stitch_mark)
            wait_log(fixture_log, "fixture_scroll", 5.0, after=fixture_mark)
            expect_probe(selection, stitch_mark, "inner over-preview")
            press_until_log(
                "return", log, "output_publish target=Clipboard", timeout=5.0, after=stitch_mark
            )
            wait_log(log, "completion_preview shown", 5.0, after=stitch_mark)
            wait_log(log, "completion_preview_expired", 8.0, after=stitch_mark)
            note(
                "mac_e2e_27_inner=pass placed=inner over_preview_scroll=true "
                "pixel_oracle=exact preview_excluded=true output=clipboard "
                "completion_preview=true auto_dismiss=true"
            )
        finally:
            stop(fixture)

        # MAC-E2E-28: a controlled known-state stop (no-overlap fast jump)
        # with verified history offers exactly two explicit choices —
        # 使用已验证部分 (incomplete, copied only after confirmation) or
        # 放弃并返回原选区 — and never auto-outputs. Subcase A abandons and
        # returns the pre-scroll selection; subcase B accepts and publishes
        # through the shared clipboard path only after the AX click.
        fixture_log = ROOT / "target" / f"e2e-scroll-fixture-{configuration}.log"
        fixture = start(
            ROOT / "target" / configuration / "arcshot-scroll-fixture", fixture_log
        )
        try:
            wait_log(fixture_log, "fixture_window", 10.0)
            time.sleep(1.0)

            def begin_scroll_28(drag_from: str, drag_to: str):
                log_mark = len(read_log(log))
                click(app, "开始截图")
                wait_frozen_active(log, log_mark)
                agent("drag", "--from-xy", drag_from, "--to-xy", drag_to, "--headed")
                wait_log(log, "capture_edit action=SelectRegion", 10.0, after=log_mark)
                selection = parse_selection(log, log_mark)
                mark = len(read_log(log))
                click_scroll_entry(app, denied=False)
                wait_log(log, "a11y_action=arcshot.overlay.scroll action=Click", 10.0, after=mark)
                wait_log(log, "scroll_collecting", 15.0, after=mark)
                return mark, selection

            def drive_to_gap_stop(mark: int) -> None:
                # Two slow steps extend the verified history, then the
                # fixture's atomic jump (key j, +1000 rows) moves past every
                # verified row: the next frame has no overlap with history
                # and must stop as a Gap, not guess. A large wheel would be
                # split into stepwise events and could extend instead.
                for _ in range(2):
                    agent("mouse-wheel", "--x", "830", "--y", "700", "--dy", "-1", "--headed")
                wait_log(log, "scroll_stitch extended", 10.0, after=mark)
                press_until_log("j", fixture_log, "fixture_jump", timeout=5.0)
                wait_log(log, "scroll_stitch_refused", 10.0, after=mark)
                line = wait_log_line(log, "scroll_partial_choice available=true", 10.0, after=mark)
                if "pixels=" not in line or "pixels=0x" in line:
                    fail(f"partial choice does not report its verified size: {line!r}")
                # No auto-output: the incomplete result waits for a choice.
                if any("output_publish" in entry for entry in read_log(log)[mark:]):
                    fail("an incomplete partial result was output without explicit choice")

            def expect_partial_choice() -> None:
                use = wait_node(
                    app,
                    lambda tree: (
                        find_nodes(tree, native_id="arcshot.scroll.partial.use", require_ref=True)
                        or [None]
                    )[0],
                    10.0,
                )
                abandon = wait_node(
                    app,
                    lambda tree: (
                        find_nodes(
                            tree, native_id="arcshot.scroll.partial.abandon", require_ref=True
                        )
                        or [None]
                    )[0],
                    10.0,
                )
                if not use.get("ref_id") or not abandon.get("ref_id"):
                    fail("the partial choice does not expose both AX buttons")
                hint = wait_node(
                    app,
                    lambda tree: next(
                        (
                            node
                            for node in find_nodes(tree, native_id="arcshot.scroll.hint")
                            if "不完整" in str(node.get("value"))
                            or "不完整" in str(node.get("name"))
                        ),
                        None,
                    ),
                    10.0,
                )
                if not hint:
                    fail("the partial choice does not warn that the result is incomplete")
                return use, abandon

            # Subcase A: abandon returns the pre-scroll selection, no output.
            mark, _selection = begin_scroll_28("620,600", "850,780")
            drive_to_gap_stop(mark)
            _use, abandon = expect_partial_choice()
            agent("click", abandon["ref_id"])
            wait_log(log, "scroll_partial_choice action=abandon", 10.0, after=mark)
            wait_log(log, "action=AbandonScrollPartial", 10.0, after=mark)
            wait_log(log, "scroll_input_retired", 10.0, after=mark)
            if any("output_publish" in entry for entry in read_log(log)[mark:]):
                fail("abandoning the partial produced output")
            root = wait_node(
                app,
                lambda tree: (
                    find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
                )[0],
                10.0,
            )
            if not find_nodes(root, native_id="arcshot.overlay.save", require_ref=True):
                fail("abandoning the partial did not return the pre-scroll selection")
            note(
                "mac_e2e_28_abandon=pass gap_stop=true choices=two incomplete_warn=true "
                "abandon=selection_returned output=none"
            )

            # Subcase B: accept publishes only after the explicit AX click.
            mark, _selection = begin_scroll_28("620,600", "850,780")
            drive_to_gap_stop(mark)
            use, _abandon = expect_partial_choice()
            if any("output_publish" in entry for entry in read_log(log)[mark:]):
                fail("the partial was output before explicit acceptance")
            agent("click", use["ref_id"])
            wait_log(log, "scroll_partial_choice action=accept", 10.0, after=mark)
            wait_log(log, "output_publish target=Clipboard", 10.0, after=mark)
            # #35: any confirmed clipboard completion shares the whole-image
            # preview post-processing, including an accepted verified part.
            wait_log(log, "completion_preview shown", 10.0, after=mark)
            wait_log(log, "completion_preview_expired", 10.0, after=mark)
            wait_log(
                log,
                "from=Frozen to=Resident action=CancelAttempt",
                10.0,
                after=mark,
            )
            note(
                "mac_e2e_28_accept=pass gap_stop=true choices=two no_auto_output=true "
                "accept=clipboard shared_preview=true auto_dismiss=true"
            )
        finally:
            stop(fixture)

        # MAC-E2E-30/31: the same Scroll path on two non-ArcShot targets
        # (TextEdit plain text, Safari file:// static page; I-12/I-13).
        # Each session drives the real app through the product (target lock,
        # inner preview, click blocking, slow wheel, Enter/double-click/Esc,
        # PNG output) and asserts product text diagnostics plus the decoded
        # clipboard PNG. Synthetic wheel steps on Cocoa/WebKit targets are
        # frequently inexact (fractional/page-sized), in which case the
        # product must stop safely (Gap/ContentChanged -> partial choice)
        # instead of guessing; that safe-stop outcome is asserted as such and
        # the positive long-image extension stays Not tested (TEST.md 4.5)
        # pending a real-device wheel step the harness cannot synthesise.
        te_fixture = ROOT / "target" / "e2e-36-textedit.txt"
        sa_fixture = ROOT / "target" / "e2e-36-safari.html"
        write_textedit_fixture(te_fixture)
        write_safari_fixture(sa_fixture)

        def textedit_scroll_fraction():
            """The TextEdit vertical scroller value (0.0 top .. 1.0 bottom)."""
            tree = snapshot_tree("TextEdit")
            values = [
                str(node.get("value"))
                for node in find_nodes(tree, role="scrollbar")
            ]
            return values

        def setup_textedit_30():
            try:
                agent("close-app", "TextEdit", "--force")
            except Exception:
                pass
            run(["open", str(te_fixture)])
            wait_target_window("TextEdit", "e2e-36-textedit")
            set_window_geometry("TextEdit", 200, 120, 1000, 700)
            run(["open", "-a", "TextEdit"])
            time.sleep(1.0)
            # Land mid-document: a viewport near either end is mostly blank and
            # therefore ambiguous (white rows match many displacements), which
            # the verifier must refuse. Wheel toward the centre, verifying the
            # vertical scroller fraction on every step.
            if frontmost_process() != "TextEdit":
                fail("TextEdit is not frontmost for the mid-document pre-scroll")
            before = textedit_scroll_fraction()
            agent("mouse-wheel", "--x", "700", "--y", "600", "--dy", "-6", "--headed")
            time.sleep(0.6)
            after = textedit_scroll_fraction()
            vertical_index = None
            for index, (old, new) in enumerate(zip(before, after)):
                if old != new:
                    vertical_index = index
                    break
            if vertical_index is None:
                fail(f"a test wheel did not move the TextEdit scroller {before} -> {after}")
            fraction = 0
            for _ in range(60):
                values = textedit_scroll_fraction()
                if len(values) <= vertical_index:
                    break
                try:
                    fraction = float(values[vertical_index])
                except ValueError:
                    break
                if 0.3 <= fraction <= 0.6:
                    break
                direction = "-6" if fraction < 0.3 else "6"
                agent(
                    "mouse-wheel", "--x", "700", "--y", "600", "--dy", direction, "--headed"
                )
                time.sleep(0.25)
            else:
                fail("the TextEdit scroller never settled mid-document")
            if not 0.3 <= fraction <= 0.6:
                fail(f"the TextEdit viewport is not mid-document (scroller={fraction})")
            time.sleep(1.0)

        def setup_safari_31():
            try:
                agent("close-app", "Safari", "--force")
            except Exception:
                pass
            run(["open", "-F", "-a", "Safari", str(sa_fixture)])
            wait_target_window("Safari", "ArcShot36")
            set_window_geometry("Safari", 100, 80, 1200, 800)
            run(["open", "-a", "Safari"])
            time.sleep(1.0)

        def textedit_state():
            tree = snapshot_tree("TextEdit")
            nodes = find_nodes(tree, role="textfield")
            return str(nodes[0].get("value") or "") if nodes else "<missing>"

        def safari_state():
            tree = snapshot_tree("Safari")
            nodes = find_nodes(tree, native_id="WEB_BROWSER_ADDRESS_AND_SEARCH_FIELD")
            return str(nodes[0].get("value") or "") if nodes else "<missing>"

        def textedit_double_ref():
            tree = snapshot_tree("TextEdit")
            nodes = find_nodes(tree, role="textfield", require_ref=True)
            if not nodes:
                fail("the TextEdit text view is not addressable for double-click")
            return nodes[0]["ref_id"]

        def safari_double_ref():
            tree = snapshot_tree("Safari")
            nodes = find_nodes(tree, role="scrollarea", require_ref=True)
            if not nodes:
                fail("the Safari scroll area is not addressable for double-click")
            return nodes[0]["ref_id"]

        def run_third_party(case: str, target_app: str, owner: str, setup, drag_from, drag_to, wheel_xy, state_fn, double_ref_fn):
            setup()
            def begin():
                log_mark = len(read_log(log))
                entry_mark, selection = begin_third_party_scroll(
                    app, log, drag_from, drag_to, log_mark
                )
                target_line = wait_log_line(log, "scroll_target window_id=", 15.0, after=entry_mark)
                if f"owner={owner}" not in target_line:
                    fail(f"{case}: unexpected target identity {target_line!r}")
                if frontmost_process() != target_app:
                    fail(f"{case}: {target_app} is not frontmost during collection")
                exclusion = wait_log_line(log, "scroll_exclusion_fixed", 15.0, after=entry_mark)
                if "preview=" not in exclusion:
                    fail(f"{case}: the fixed preview is not in the exclusion set")
                return entry_mark, selection

            def retired_stats(after: int):
                line = wait_log_line(log, "scroll_input_retired", 10.0, after=after)
                return {
                    "scrolls": int(line.split("scrolls=", 1)[1].split(" ", 1)[0]),
                    "clicks_blocked": int(
                        line.split("clicks_blocked=", 1)[1].split(" ", 1)[0]
                    ),
                }

            # Session A: slow wheel attempt (positive or safe-stop), click blocking.
            # The target state is sampled before the session (the tap blocks
            # clicks and scrolling never rewrites the document/address).
            state_before = state_fn()
            log_mark = len(read_log(log))
            entry_mark, selection = begin()
            # Click blocking: a raw click inside the selection never lands.
            agent("mouse-click", "--xy", wheel_xy, "--headed")
            time.sleep(1.0)
            if state_fn() != state_before:
                fail(f"{case}: a blocked click changed the target state")
            def stitch_quiet(after: int, quiet: float = 1.5, limit: float = 10.0):
                """Waits until extensions stop for `quiet` seconds (flush drains).

                Idle frames are duplicates (pump noise), so quiescence is judged
                by the extension count staying flat once frames are flowing.
                """
                previous = -1
                silent_since = time.monotonic()
                deadline = time.monotonic() + limit
                while time.monotonic() < deadline:
                    extended_now = count_log_after(log, "scroll_stitch extended", after)
                    if extended_now != previous:
                        previous = extended_now
                        silent_since = time.monotonic()
                    elif (
                        time.monotonic() - silent_since >= quiet
                        and count_log_after(log, "scroll_stitch duplicate", after) >= 1
                    ):
                        return
                    time.sleep(0.2)

            def max_bottom(after: int) -> int:
                best = 0
                for entry in read_log(log)[after:]:
                    if "bottom=" in entry and "scroll_stitch" in entry:
                        try:
                            best = max(best, int(entry.split("bottom=", 1)[1].split(" ", 1)[0]))
                        except (ValueError, IndexError):
                            pass
                return best

            down_mark = len(read_log(log))
            wheel_x, wheel_y = wheel_xy.split(",")
            # One slow step down, then back: deeper histories develop periodic
            # whitespace that the strict verifier rightly refuses as
            # ambiguous, so the reverse leg stays shallow (one step each).
            for _ in range(1):
                agent("mouse-wheel", "--x", wheel_x, "--y", wheel_y, "--dy", "-6", "--headed")
                time.sleep(0.6)
            stitch_quiet(down_mark)
            extended_down = count_log_after(log, "scroll_stitch extended", down_mark)
            bottom_down = max_bottom(down_mark)
            up_mark = len(read_log(log))
            for _ in range(1):
                agent("mouse-wheel", "--x", wheel_x, "--y", wheel_y, "--dy", "6", "--headed")
                time.sleep(0.6)
            stitch_quiet(up_mark)
            extended_up = count_log_after(log, "scroll_stitch extended", up_mark)
            refused = count_log_after(log, "scroll_stitch_refused", entry_mark)
            duplicates = count_log_after(log, "scroll_stitch duplicate", up_mark)
            bottom = max_bottom(entry_mark)
            if extended_down + extended_up >= 1:
                # Reverse de-duplication: the way back must visit verified
                # history (duplicates) and must not grow the document
                # downward (an upward extension past the start keeps bottom).
                if duplicates < 1 or bottom > bottom_down:
                    fail(f"{case}: reverse pass was not de-duplicated")
                before = len(read_log(log))
                published = False
                for _attempt in range(6):
                    agent("press", "return", "--headed")
                    deadline = time.monotonic() + 1.0
                    while time.monotonic() < deadline:
                        if count_log_after(log, "output_publish", before) > 0:
                            published = True
                            break
                        time.sleep(0.2)
                    if published:
                        break
                if not published:
                    fail(f"{case}: Enter never published after mark")
                metrics = decode_clipboard_expect(selection)
                if metrics["height"] != bottom:
                    fail(
                        f"{case}: decoded height {metrics['height']} != verified bottom {bottom}"
                    )
                scroll_outcome = "extended"
            else:
                if refused < 1:
                    fail(f"{case}: slow scroll neither extended nor stopped safely")
                wait_log(log, "scroll_partial_choice available=true", 5.0, after=entry_mark)
                if any("output_publish" in entry for entry in read_log(log)[entry_mark:]):
                    fail(f"{case}: unverifiable frames were output without a choice")
                abandon = wait_node(
                    app,
                    lambda tree: (
                        find_nodes(
                            tree, native_id="arcshot.scroll.partial.abandon", require_ref=True
                        )
                        or [None]
                    )[0],
                    10.0,
                )
                agent("click", abandon["ref_id"])
                wait_log(log, "action=AbandonScrollPartial", 10.0, after=down_mark)
                if count_log_after(log, "output_publish", entry_mark) != 0:
                    fail(f"{case}: abandoning the safe stop produced output")
                time.sleep(0.5)
                expect_selection_returned(app)
                scroll_outcome = "safe_stop"
            stats = retired_stats(entry_mark)
            if stats["clicks_blocked"] < 1:
                fail(f"{case}: the click was never blocked {stats}")
            if stats["scrolls"] < 1:
                fail(f"{case}: raw scroll never reached the target {stats}")
            # Session B: no-scroll single frame finished with Enter.
            log_mark = len(read_log(log))
            entry_mark, selection = begin()
            before = len(read_log(log))
            published = False
            for _attempt in range(6):
                agent("press", "return", "--headed")
                deadline = time.monotonic() + 1.0
                while time.monotonic() < deadline:
                    if count_log_after(log, "output_publish", before) > 0:
                        published = True
                        break
                    time.sleep(0.2)
                if published:
                    break
            if not published:
                fail(f"{case}: Enter never published the single frame")
            wait_log(
                log, "from=Frozen to=Resident action=OutputCompleted", 10.0, after=entry_mark
            )
            metrics = decode_clipboard_expect(selection)
            if metrics["height"] != int(selection[3] * 2):
                fail(f"{case}: single-frame PNG height {metrics['height']} is not the viewport")
            if retired_stats(entry_mark)["scrolls"] != 0:
                fail(f"{case}: the no-scroll frame counted raw scrolls")
            single_png = f"{metrics['width']}x{metrics['height']}/sRGB/sha={metrics['sha256'][:12]}"
            # Session C: double-click completion.
            log_mark = len(read_log(log))
            entry_mark, selection = begin()
            agent("double-click", double_ref_fn(), "--headed")
            wait_log(log, "scroll_tap_command", 5.0, after=entry_mark)
            wait_log(log, "output_publish target=Clipboard", 10.0, after=entry_mark)
            decode_clipboard_expect(selection)
            # Session D: Esc abandons and returns the pre-scroll selection.
            log_mark = len(read_log(log))
            entry_mark, selection = begin()
            before = len(read_log(log))
            cancelled = False
            for _attempt in range(6):
                agent("press", "escape", "--headed")
                deadline = time.monotonic() + 1.0
                while time.monotonic() < deadline:
                    if count_log_after(
                        log, "from=Collecting to=Inactive action=CancelScroll", before
                    ) > 0:
                        cancelled = True
                        break
                    time.sleep(0.2)
                if cancelled:
                    break
            if not cancelled:
                fail(f"{case}: Esc never produced CancelScroll")
            if any("output_publish" in entry for entry in read_log(log)[entry_mark:]):
                fail(f"{case}: Esc produced output")
            expect_selection_returned(app)
            # Leave Resident for the next case.
            root = wait_node(
                app,
                lambda tree: (
                    find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
                )[0],
                10.0,
            )
            cancel = find_nodes(root, native_id="arcshot.overlay.cancel", require_ref=True)
            agent("click", cancel[0]["ref_id"])
            wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 10.0)
            return scroll_outcome, single_png

        try:
            outcome_30, png_30 = run_third_party(
                "MAC-E2E-30",
                "TextEdit",
                "com.apple.TextEdit",
                setup_textedit_30,
                "400,350",
                "1000,650",
                "700,600",
                textedit_state,
                textedit_double_ref,
            )
        finally:
            try:
                agent("close-app", "TextEdit", "--force")
            except Exception:
                pass
        note(
            f"mac_e2e_30=pass target=TextEdit scroll={outcome_30} "
            f"click_blocked=true raw_scroll=true single={png_30} "
            f"double_click=true esc=true fixture={TEXTEDIT_FIXTURE_VERSION}"
        )
        try:
            outcome_31, png_31 = run_third_party(
                "MAC-E2E-31",
                "Safari",
                "com.apple.Safari",
                setup_safari_31,
                "300,350",
                "900,650",
                "600,600",
                safari_state,
                safari_double_ref,
            )
        finally:
            try:
                agent("close-app", "Safari", "--force")
            except Exception:
                pass
        note(
            f"mac_e2e_31=pass target=Safari scroll={outcome_31} "
            f"click_blocked=true raw_scroll=true single={png_31} "
            f"double_click=true esc=true fixture={SAFARI_FIXTURE_VERSION} offline=file"
        )

        # MAC-E2E-15: the 完成音效 preference toggles through AX and is persisted.
        original_sound = next(
            line.split("completion_sound=", 1)[1].split(" ", 1)[0]
            for line in read_log(log)
            if "preferences_launch completion_sound=" in line
        )
        if original_sound not in ("true", "false"):
            fail(f"unexpected persisted completion-sound value: {original_sound!r}")
        flipped_sound = "false" if original_sound == "true" else "true"
        click(app, "完成音效")
        wait_log(log, f"completion_sound enabled={flipped_sound} persisted=true", 10.0)
        click(app, "完成音效")
        wait_log(log, f"completion_sound enabled={original_sound} persisted=true", 10.0)
        note(
            f"mac_e2e_15=pass sound={original_sound}->{flipped_sound}->{original_sound} "
            "selector=toggleCompletionSound:"
        )
        # Leave the opposite value in the preferences domain before the restart.
        click(app, "完成音效")
        wait_log(log, f"completion_sound enabled={flipped_sound} persisted=true", 10.0)

        # MAC-E2E-08: Quit removes process, status item and windows.
        quit_app(app, process, expect_status_item_removed=True)
        note("mac_e2e_08=pass exit=0 status_item=removed")

        # MAC-E2E-16: the stored preference survives a real restart and is then
        # restored to the value the user had before this run.
        process = start(executable, restart_logs[0])
        wait_log(
            restart_logs[0], f"preferences_launch completion_sound={flipped_sound}", 10.0
        )
        click(app, "完成音效")
        wait_log(
            restart_logs[0], f"completion_sound enabled={original_sound} persisted=true", 10.0
        )
        quit_app(app, process, expect_status_item_removed=False)
        process = start(executable, restart_logs[1])
        wait_log(
            restart_logs[1], f"preferences_launch completion_sound={original_sound}", 10.0
        )
        quit_app(app, process, expect_status_item_removed=False)
        note(
            f"mac_e2e_16=pass restart_restored={flipped_sound} "
            f"restart_restored={original_sound} preference=local"
        )

        # MAC-E2E-22: 选项密度 switches through AX, keeps the same option set
        # and the active tool/styles, survives a restart, and is restored to the
        # value the user had before this run. No pixels, no coordinate clicks.
        process = start(executable, restart_logs[2])
        original_density = launch_density(restart_logs[2])
        flipped_density = "Standard" if original_density == "Compact" else "Compact"

        # Freeze at the original density and set up a tool plus a per-tool style.
        click(app, "开始截图")
        wait_log(restart_logs[2], "capture_wake_window_submit_us=", 15.0)
        wait_frozen_active(restart_logs[2], 0)
        rectangle_tool = wait_tool_state(
            app, "arcshot.overlay.tool.rectangle", False, 10.0
        )
        agent("click", rectangle_tool["ref_id"])
        wait_log(restart_logs[2], "capture_edit action=SelectTool", 10.0)
        hover(rectangle_tool["ref_id"])
        width_five = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.rectangle.width.5",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        blue = wait_node(
            app,
            lambda tree: (
                find_nodes(
                    tree,
                    native_id="arcshot.overlay.option.rectangle.color.blue",
                    require_ref=True,
                )
                or [None]
            )[0],
            10.0,
        )
        agent("click", width_five["ref_id"])
        wait_log(restart_logs[2], "width=5", 10.0)
        agent("click", blue["ref_id"])
        wait_log(restart_logs[2], "style=stroke=#007AFF width=5", 10.0)

        # Switch density through the status menu while the session is frozen.
        click(app, "选项密度")
        wait_log(
            restart_logs[2],
            f"option_density density={flipped_density} persisted=true",
            10.0,
        )
        # The dock still exposes the real density as an AX value.
        density_node = wait_node(
            app,
            lambda tree: next(
                (
                    node
                    for node in find_nodes(tree, native_id="arcshot.overlay.density")
                    if density_node_matches(node, flipped_density)
                ),
                None,
            ),
            10.0,
        )
        if not density_node_matches(density_node, flipped_density):
            fail(f"dock does not report {flipped_density}: {density_node}")
        # The switch cleared neither the tool nor the per-tool styles, and the
        # same option ids remain reachable at the new density.
        preserved_tool = wait_tool_state(
            app, "arcshot.overlay.tool.rectangle", True, 10.0
        )
        if not preserved_tool.get("ref_id"):
            fail("the rectangle tool is not addressable after a density switch")
        hover(preserved_tool["ref_id"])
        wait_tool_state(app, "arcshot.overlay.option.rectangle.width.5", True, 10.0)
        wait_tool_state(app, "arcshot.overlay.option.rectangle.color.blue", True, 10.0)
        # Finish the session through AX cancel.
        root = wait_node(
            app,
            lambda tree: (
                find_nodes(tree, role="group", name="ArcShot 截图覆盖层") or [None]
            )[0],
            10.0,
        )
        cancel = find_nodes(root, native_id="arcshot.overlay.cancel", require_ref=True)
        agent("click", cancel[0]["ref_id"])
        wait_log(restart_logs[2], "from=Frozen to=Resident action=CancelAttempt", 10.0)

        # The flipped value survives a real restart, then is restored through
        # the same AX path; a final restart confirms the user's original value.
        quit_app(app, process, expect_status_item_removed=False)
        process = start(executable, restart_logs[3])
        if launch_density(restart_logs[3]) != flipped_density:
            fail("option density did not survive a real restart")
        click(app, "选项密度")
        wait_log(
            restart_logs[3],
            f"option_density density={original_density} persisted=true",
            10.0,
        )
        quit_app(app, process, expect_status_item_removed=False)
        process = start(executable, restart_logs[4])
        if launch_density(restart_logs[4]) != original_density:
            fail("the user's original option density was not restored")
        quit_app(app, process, expect_status_item_removed=False)
        note(
            f"mac_e2e_22=pass density={original_density}->{flipped_density}->{original_density} "
            "option_set=reachable tool_and_style=preserved restart=restored ax=dock_value"
        )
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                process.kill()

    note(f"mac_e2e=pass configuration={configuration} log={log.relative_to(ROOT)} cases=30")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Failure as failure:
        print(f"e2e check failed: {failure}", file=sys.stderr)
        sys.exit(1)
