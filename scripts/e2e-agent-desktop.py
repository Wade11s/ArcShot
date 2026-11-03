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
import json
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
POLL = 0.1

# Menu order is part of the product contract (crates/macos MenuBar::try_new).
MENU = [
    ("开始截图", "startCapture:"),
    ("截图快捷键…", "hotkeySettings:"),
    ("屏幕录制权限…", "screenRecordingPermissions:"),
    ("退出 ArcShot", "terminate:"),
]

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


def count_log(path: pathlib.Path, needle: str) -> int:
    return sum(1 for line in read_log(path) if needle in line)


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


def press(combo: str, *, headed: bool) -> None:
    args = ["press", combo] + (["--headed"] if headed else [])
    data = agent(*args)
    steps = data.get("steps", [])
    if not steps or steps[0].get("outcome") != "succeeded":
        fail(f"press {combo} did not report success: {data}")


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

    build = run([str(ROOT / "scripts" / "build-app.sh")] + (["--release"] if options.release else []))
    if build.returncode != 0:
        fail(f"build failed: {build.stderr.strip()[-600:]}")
    actual = run(["plutil", "-extract", "CFBundleIdentifier", "raw", str(bundle / "Contents" / "Info.plist")]).stdout.strip()
    actual_name = run(["plutil", "-extract", "CFBundleName", "raw", str(bundle / "Contents" / "Info.plist")]).stdout.strip()
    if (actual, actual_name) != (identifier, app):
        fail(f"bundle identity mismatch: {actual} / {actual_name}, expected {identifier} / {app}")

    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text("")
    stderr = log.open("w")
    process = subprocess.Popen([str(executable)], stdout=subprocess.DEVNULL, stderr=stderr, cwd=ROOT)
    stderr.close()
    try:
        wait_log(log, "hotkey_launch config=", 10.0)

        # MAC-E2E-09: the running instance is the bundle under test.
        apps = agent("list-apps", "--app", "ArcShot").get("apps", [])
        identities = sorted((entry.get("bundle_id"), entry.get("name")) for entry in apps)
        if identities != [(identifier, app)]:
            fail(f"unexpected running instances: {identities}")
        note(f"mac_e2e_09=pass bundle_id={identifier} name={app}")

        # MAC-E2E-01: status item and the four menu commands, without taking focus.
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

        # MAC-E2E-02: a resident instance exposes no own window.
        if windows(app):
            fail(f"resident instance exposes own windows: {windows(app)}")
        note("mac_e2e_02=pass own_windows=0")

        # MAC-E2E-03: the menu command enters the session and freezes the screen.
        click(app, "开始截图")
        wait_log(log, "capture_request_us=", 15.0)
        wait_log(log, "capture_wake_window_submit_us=", 15.0)
        wait_log(log, "to=Frozen action=Completed", 15.0)
        if windows(app):
            fail(f"frozen session exposes a capturable own window: {windows(app)}")
        note("mac_e2e_03=pass menu_start=frozen own_windows=0")

        # MAC-E2E-05a/05b: Escape reaches the app and the session leaves Frozen.
        press("escape", headed=True)
        wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 10.0)
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
        press(hotkey_combo(config), headed=False)
        wait_count(log, "capture_request_us=", before + 1, 10.0)
        if count_log(log, "capture_request_us=") != before + 1:
            fail("the registered hotkey did not start exactly one capture")
        wait_log(log, "to=Frozen action=Completed", 15.0)
        note(f"mac_e2e_04=pass hotkey={config} negative_control=no_capture")
        press("escape", headed=True)
        wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 10.0)

        # MAC-E2E-07: the permission notice appears and Escape closes it.
        click(app, "屏幕录制权限…")
        notice = wait_window_size(app, 560.0, 282.0, 10.0)
        if not notice.get("is_focused"):
            fail(f"permission notice is not focused, refusing headed key delivery: {notice}")
        size = notice.get("bounds", {})
        press("escape", headed=True)
        wait_windows(app, 0, 10.0)
        note(f"mac_e2e_07=pass notice_size={size['width']}x{size['height']} escape_closed=true")

        # MAC-E2E-06: the hotkey settings window opens as an addressable window.
        click(app, "截图快捷键…")
        wait_log(log, "menu_hotkey_settings", 10.0)
        settings = wait_window_size(app, 480.0, 362.0, 10.0)
        size = settings.get("bounds", {})
        note(f"mac_e2e_06=pass settings_size={size['width']}x{size['height']}")

        # MAC-E2E-08: Quit removes process, status item and windows.
        click(app, "退出 ArcShot")
        try:
            status_code = process.wait(timeout=10.0)
        except subprocess.TimeoutExpired:
            fail("process did not exit after the quit command")
        if status_code != 0:
            fail(f"process exited {status_code} after the quit command")
        if agent("list-apps", "--app", "ArcShot").get("apps"):
            fail("an ArcShot instance is still running after quit")
        try:
            agent("find", "--surface", "menu-bar-extras", "--app", app, "--role", "menuitem")
        except Failure as error:
            if "APP_NOT_FOUND" not in str(error):
                raise
        else:
            fail("status item is still addressable after quit")
        note("mac_e2e_08=pass exit=0 status_item=removed")
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                process.kill()

    note(f"mac_e2e=pass configuration={configuration} log={log.relative_to(ROOT)} cases=9")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Failure as failure:
        print(f"e2e check failed: {failure}", file=sys.stderr)
        sys.exit(1)
