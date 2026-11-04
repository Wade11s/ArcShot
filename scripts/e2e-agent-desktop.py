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
    ("完成音效", "toggleCompletionSound:"),
    ("登录时启动", "toggleLaunchAtLogin:"),
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
            # that is a race in the observation, not a product failure.
            if "WINDOW_NOT_FOUND" not in str(error):
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

    log = ROOT / "target" / f"e2e-agent-desktop-{configuration}.log"
    process = start(executable, log)
    restart_logs = [
        ROOT / "target" / f"e2e-agent-desktop-{configuration}-restart-{index}.log"
        for index in (1, 2)
    ]
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
        log_mark = len(read_log(log))
        press(hotkey_combo(config), headed=False)
        wait_count(log, "capture_request_us=", before + 1, 10.0)
        if count_log(log, "capture_request_us=") != before + 1:
            fail("the registered hotkey did not start exactly one capture")
        wait_frozen_active(log, log_mark)
        wait_log(log, "to=Frozen action=Completed", 15.0, after=log_mark)
        note(f"mac_e2e_04=pass hotkey={config} negative_control=no_capture")
        press("escape", headed=True)
        wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 10.0, after=log_mark)

        # MAC-E2E-07: the permission notice appears and Escape closes it.
        click(app, "屏幕录制权限…")
        notice = wait_window_size(app, 560.0, 282.0, 10.0)
        if not notice.get("is_focused"):
            fail(f"permission notice is not focused, refusing headed key delivery: {notice}")
        wait_frontmost(app, 5.0)
        size = notice.get("bounds", {})
        press("escape", headed=True)
        wait_windows(app, 0, 10.0)
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
        press(hotkey_combo(temporary), headed=False)
        wait_log(log, f"candidate={temporary}", 10.0)
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
        press(hotkey_combo(original), headed=False)
        wait_log(log, f"candidate={original}", 10.0)
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
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                process.kill()

    note(f"mac_e2e=pass configuration={configuration} log={log.relative_to(ROOT)} cases=21")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Failure as failure:
        print(f"e2e check failed: {failure}", file=sys.stderr)
        sys.exit(1)
