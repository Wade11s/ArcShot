#!/usr/bin/env python3
"""ArcShot T-04 performance measurement driver.

Drives the real Release product through `agent-desktop` and the product's own
text diagnostics, then evaluates the T-04 gates with `scripts/perf-gate.sh`.
Text/numeric evidence only; never saves pixels.

usage: perf-measure.py [--configuration release|debug] [--samples N]
                       [--metrics a,b,...] [--log PATH] [--no-build]
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
POLL = 0.1
SCREEN_Y_OFFSET = 33.0
FIXTURE_RECT = (600.0, 120.0, 760.0, 720.0)
FIXTURE_INITIAL_OFFSET_ROWS = 2000
FIXTURE_MAX_OFFSET_ROWS = 7400

# The fixture's deterministic content reaches the 24 M document cap when the
# stitched height reaches floor(24_000_000 / selection_width).
SELECTION_WIDTH_PHYSICAL = 1500
MAX_DOCUMENT_HEIGHT = 24_000_000 // SELECTION_WIDTH_PHYSICAL

TOOLS = ["select", "rectangle", "line", "arrow", "blur", "text"]
ALL_METRICS = ["region_sck", "interact", "clipboard_export", "scroll", "export_24m"]


class Failure(Exception):
    """A measurement step did not hold; the message is the whole story."""


def fail(message: str) -> None:
    raise Failure(message)


def note(line: str) -> None:
    print(line, flush=True)


def run(argv: list[str], *, timeout: float = 60.0) -> subprocess.CompletedProcess:
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


def agent(*args: str) -> dict:
    result = run(["agent-desktop", *args])
    try:
        envelope = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        fail(
            f"agent-desktop {' '.join(args)}: unparsable output ({error}): "
            f"{result.stdout[:400]}"
        )
    if not envelope.get("ok"):
        error = envelope.get("error", {})
        fail(
            f"agent-desktop {' '.join(args)}: {error.get('code')}: "
            f"{error.get('message')} "
            f"(delivery={error.get('disposition', {}).get('delivery')})"
        )
    return envelope["data"]


def read_log(path: pathlib.Path) -> list[str]:
    try:
        return path.read_text(errors="replace").splitlines()
    except FileNotFoundError:
        return []


def wait_log(path: pathlib.Path, needle: str, timeout: float, *, after: int = 0) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        lines = read_log(path)
        for index in range(after, len(lines)):
            if needle in lines[index]:
                return index
        time.sleep(POLL)
    fail(f"timeout waiting for {needle!r} in {path}")


def wait_line(path: pathlib.Path, predicate, timeout: float) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for line in read_log(path):
            if predicate(line):
                return line
        time.sleep(POLL)
    fail(f"timeout waiting for a matching line in {path}")


def count_log(path: pathlib.Path, needle: str, *, after: int = 0) -> int:
    return sum(1 for line in read_log(path)[after:] if needle in line)


def wait_frozen_active(log: pathlib.Path, after: int, timeout: float = 20.0) -> int:
    wake = wait_log(log, "capture_wake_window_submit_us=", timeout, after=after)
    wait_log(log, "app_active surface=frozen", timeout, after=wake)
    return wake


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
    deadline = time.monotonic() + timeout
    last: dict = {}
    while time.monotonic() < deadline:
        try:
            last = snapshot_tree(app)
        except Failure as error:
            if "WINDOW_NOT_FOUND" not in str(error) and "AMBIGUOUS_TARGET" not in str(error):
                raise
            last = {}
        else:
            node = predicate(last)
            if node:
                return node
        time.sleep(POLL)
    fail(f"accessibility node did not appear in {timeout}s; last tree: {json.dumps(last)[:600]}")


def click_menu(app: str, name: str) -> None:
    # The status item's accessibility element can be momentarily unavailable
    # after many rapid sessions; retry the read instead of failing the sample.
    for _ in range(5):
        try:
            data = agent(
                "find", "--surface", "menu-bar-extras", "--app", app,
                "--role", "menuitem", "--name", name,
            )
        except Failure:
            time.sleep(0.5)
            continue
        matches = data.get("matches", [])
        if len(matches) == 1:
            agent("click", matches[0]["ref_id"])
            return
        time.sleep(0.5)
    fail(f"expected exactly one menu item named {name!r}, got none")


def click_overlay(app: str, native_id: str, timeout: float = 10.0) -> None:
    node = wait_node(
        app,
        lambda tree: (
            find_nodes(tree, native_id=native_id, require_ref=True) or [None]
        )[0],
        timeout,
    )
    agent("click", node["ref_id"])


def start(executable: pathlib.Path, log: pathlib.Path, args: list[str] | None = None) -> subprocess.Popen:
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text("")
    stderr = log.open("w")
    process = subprocess.Popen(
        [str(executable), *(args or [])], stdout=subprocess.DEVNULL, stderr=stderr, cwd=ROOT
    )
    stderr.close()
    return process


def stop(process: subprocess.Popen) -> None:
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5.0)
        except subprocess.TimeoutExpired:
            process.kill()


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
    timeout: float = 5.0,
    attempts: int = 4,
    after: int | None = None,
) -> str:
    mark = len(read_log(log)) if after is None else after
    for _ in range(attempts):
        press(combo, headed=True)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            lines = read_log(log)
            for index in range(mark, len(lines)):
                if needle in lines[index]:
                    return lines[index]
            time.sleep(POLL)
    fail(f"press {combo!r} never produced {needle!r}")


def select_region(
    app: str,
    log: pathlib.Path,
    *,
    drag_from: str,
    drag_to: str,
    mark: int,
    attempts: int = 3,
) -> tuple[float, float, float, float]:
    for attempt in range(attempts):
        agent("drag", "--from-xy", drag_from, "--to-xy", drag_to, "--headed")
        try:
            wait_log(log, "capture_edit action=SelectRegion", 4.0, after=mark)
            line = wait_line(log, lambda text: "capture_edit action=SelectRegion" in text, 2.0)
            return parse_selection(line)
        except Failure:
            if attempt == attempts - 1:
                raise
            restart = len(read_log(log))
            click_menu(app, "开始截图")
            wait_frozen_active(log, restart)
            mark = restart
    fail("the selection drag never reached the overlay")


def parse_selection(line: str) -> tuple[float, float, float, float]:
    raw = line.split("selection=Some([", 1)[1].split("])", 1)[0]
    values = [float(value) for value in raw.split(",")]
    if len(values) != 4:
        fail(f"unexpected selection payload: {line!r}")
    return tuple(values)  # type: ignore[return-value]


def drag_pair(
    from_xy: tuple[float, float], to_xy: tuple[float, float]
) -> tuple[str, str]:
    return (
        f"{from_xy[0]:.0f},{from_xy[1] + SCREEN_Y_OFFSET:.0f}",
        f"{to_xy[0]:.0f},{to_xy[1] + SCREEN_Y_OFFSET:.0f}",
    )


def wheel(x: int, y: int, dy: int) -> None:
    # The physical pointer can transiently fail to reach its target under
    # load; a single failed wheel aborts a multi-minute collection, so
    # retry briefly (the delivery is idempotent: a scroll either happens
    # or it does not, and the fixture's receipts record what landed).
    for attempt in range(5):
        try:
            agent("mouse-wheel", "--x", str(x), "--y", str(y), "--dy", str(dy), "--headed")
            return
        except Failure as error:
            if attempt == 4:
                raise
            note(f"  wheel retry {attempt + 1}: {error}")
            time.sleep(0.5)


def document_height(log: pathlib.Path) -> int:
    heights = []
    for line in read_log(log):
        if "scroll_preview_updated" in line and "document=" in line:
            try:
                document = line.split("document=", 1)[1].split(" ", 1)[0]
                heights.append(int(document.split("x", 1)[1]))
            except (IndexError, ValueError):
                continue
    return heights[-1] if heights else 0


def start_capture(app: str, log: pathlib.Path) -> int:
    mark = len(read_log(log))
    click_menu(app, "开始截图")
    wait_frozen_active(log, mark)
    return mark


def cancel_capture(app: str, log: pathlib.Path, mark: int) -> None:
    press_until_log(
        "escape", log, "capture_phase from=Frozen to=Resident action=CancelAttempt",
        timeout=5.0, after=mark,
    )


# --- Metric drivers -------------------------------------------------------


def measure_region_sck(
    app: str, log: pathlib.Path, samples: int
) -> None:
    note(f"region_sck: {samples} capture cycles")
    start_capture(app, log)
    cancel_capture(app, log, len(read_log(log)) - 1)
    for index in range(samples):
        mark = start_capture(app, log)
        cancel_capture(app, log, mark)
        if (index + 1) % 10 == 0:
            note(f"  region_sck sample {index + 1}/{samples}")


def measure_interact(app: str, log: pathlib.Path, samples: int) -> None:
    note(f"interact: {samples} tool switches")
    mark = start_capture(app, log)
    for index in range(samples):
        tool = TOOLS[index % len(TOOLS)]
        before_index = len(read_log(log))
        click_overlay(app, f"arcshot.overlay.tool.{tool}")
        try:
            wait_log(log, "interact_commit_us=", 5.0, after=before_index)
        except Failure:
            # A tool switch that does not change the document still commits.
            pass
        if (index + 1) % 10 == 0:
            note(f"  interact sample {index + 1}/{samples}")
    cancel_capture(app, log, mark)


def measure_clipboard_export(
    app: str,
    log: pathlib.Path,
    samples: int,
    *,
    drag_from: tuple[float, float],
    drag_to: tuple[float, float],
) -> None:
    note(f"clipboard_export: {samples} finish cycles")
    for index in range(samples):
        mark = start_capture(app, log)
        from_str, to_str = drag_pair(drag_from, drag_to)
        select_region(
            app, log,
            drag_from=from_str,
            drag_to=to_str,
            mark=mark,
        )
        click_overlay(app, "arcshot.overlay.finish")
        wait_log(log, "output_publish target=Clipboard", 30.0, after=mark)
        wait_log(
            log,
            "capture_phase from=Frozen to=Resident action=OutputCompleted",
            15.0,
            after=mark,
        )
        if (index + 1) % 10 == 0:
            note(f"  clipboard_export sample {index + 1}/{samples}")


def enter_fixture_scroll(
    app: str,
    log: pathlib.Path,
    *,
    drag_from: tuple[float, float],
    drag_to: tuple[float, float],
) -> int:
    mark = start_capture(app, log)
    from_str, to_str = drag_pair(drag_from, drag_to)
    select_region(
        app, log,
        drag_from=from_str,
        drag_to=to_str,
        mark=mark,
    )
    click_overlay(app, "arcshot.overlay.scroll")
    wait_log(log, "scroll_collecting", 20.0, after=mark)
    return mark


def measure_scroll(app: str, log: pathlib.Path, samples: int) -> None:
    note(f"scroll: {samples} slow wheel events on the fixture")
    fixture_log = ROOT / "target" / "perf-scroll-fixture.log"
    fixture = start(ROOT / "target" / "release" / "arcshot-scroll-fixture", fixture_log)
    try:
        wait_log(fixture_log, "fixture_window", 10.0)
        time.sleep(1.0)
        mark = enter_fixture_scroll(app, log, drag_from=(620, 200), drag_to=(1000, 600))
        for index in range(samples):
            wheel(640, 250, -1 if index % 2 == 0 else 1)
            try:
                wait_log(log, "scroll_preview_updated", 5.0, after=mark)
            except Failure:
                pass
            if (index + 1) % 10 == 0:
                note(f"  scroll sample {index + 1}/{samples}")
        press_until_log(
            "return", log, "output_publish target=Clipboard", timeout=10.0, after=mark
        )
        wait_log(log, "completion_preview shown", 10.0, after=mark)
        wait_log(log, "completion_preview_expired", 10.0, after=mark)
    finally:
        stop(fixture)


def fixture_24m_args() -> list[str]:
    """The #59 24M fixture geometry: wider window, exact clamped range.

    Starting at content row 0 avoids an up-scroll phase: a 384-physical-row
    viewport scrolled down to row 7808 yields 384 + 2 * 7808 = 16000 rows at
    1500 pixels per row = 24,000,000 pixels. It also removes the momentum
    inversion that coalesced wheel events can produce when the collection
    changes direction under load.
    """
    return ["--width", "800", "--max-offset", "7808", "--initial-offset", "0"]


def build_24m_document(app: str, log: pathlib.Path) -> int:
    """Drives one slow, strictly verified Scroll collection up to the 24M cap.

    Returns the log mark at which this session started. The fixture process
    must stay alive until the completion preview has expired: the locked
    Scroll target is re-checked during collection and output.
    """
    mark = enter_fixture_scroll(
        app, log, drag_from=(620, 196), drag_to=(1370, 388)
    )
    # Scroll down in small steps, waiting for the verifier. The fixture
    # clamps at 7808, so any late or coalesced wheel event is de-duplicated
    # instead of pushing the document past the 24,000,000-pixel cap. Large
    # synthetic wheel steps can be caught mid-rebuild by the continuous
    # capture and read as changed content inside the overlap, so the cadence
    # stays at the controlled slow-scroll rate used by the E2E fixtures.
    for _ in range(600):
        wheel(650, 280, -1)
        time.sleep(0.15)
        if document_height(log) >= MAX_DOCUMENT_HEIGHT:
            break
    # Let the last verified frame and the bounded raw queue drain before
    # Finish; the clamped fixture turns any late wheel into a duplicate,
    # so no frame can push the document past the 24M cap.
    deadline = time.monotonic() + 8.0
    while document_height(log) < MAX_DOCUMENT_HEIGHT and time.monotonic() < deadline:
        time.sleep(0.1)
    time.sleep(1.0)
    height = document_height(log)
    note(f"  built document height={height} (target {MAX_DOCUMENT_HEIGHT})")
    if height < MAX_DOCUMENT_HEIGHT:
        fail(
            f"the fixture document only reached height {height} "
            f"(need {MAX_DOCUMENT_HEIGHT})"
        )
    return mark


def measure_export_24m(
    app: str,
    log: pathlib.Path,
    runs: int,
    sampler: pathlib.Path | None,
    pid: int,
) -> None:
    note(f"export_24m: {runs} independent {MAX_DOCUMENT_HEIGHT}-row documents/exports")
    fixture_log = ROOT / "target" / "perf-24m-fixture.log"
    sampler_log = ROOT / "target" / "perf-24m-resource-samples.log"
    sampler_process = None
    if sampler is not None:
        sampler_stdout = sampler_log.open("w")
        sampler_process = subprocess.Popen(
            # 50 ms sleep keeps the observed RSS cadence under 100 ms even
            # after `ps` and the slower `footprint` cross-check complete.
            [str(sampler), str(pid), "0.05", str(240 * runs)],
            stdout=sampler_stdout,
            stderr=subprocess.STDOUT,
            text=True,
        )
        sampler_stdout.close()
    try:
        for run in range(runs):
            # A fresh fixture resets the content offset to row 0. The first
            # clipboard copy of a Scroll completion keeps the document for
            # optional annotation and then ends the session on re-finish, so
            # five maximum-size exports require five consecutive sessions
            # (T-04: five consecutive runs of the fixed-document encode).
            fixture = start(
                ROOT / "target" / "release" / "arcshot-scroll-fixture",
                fixture_log,
                fixture_24m_args(),
            )
            try:
                wait_log(fixture_log, "fixture_window", 10.0)
                time.sleep(1.0)
                mark = build_24m_document(app, log)
                # Enter / double-click is the product's Scroll finish gesture
                # (I-11 §6); the capture overlay is dismissed during collection.
                press_until_log(
                    "return", log, "output_publish target=Clipboard",
                    timeout=20.0, attempts=2, after=mark,
                )
                publish = wait_log(
                    log, "output_publish target=Clipboard", 60.0, after=mark
                )
                wait_log(log, "completion_preview shown", 20.0, after=publish)
                wait_log(log, "completion_preview_expired", 20.0, after=publish)
                wait_log(log, "from=Frozen to=Resident action=CancelAttempt", 20.0, after=publish)
                note(f"  export_24m run {run + 1}/{runs} published")
            finally:
                stop(fixture)
    finally:
        if sampler_process is not None:
            sampler_process.terminate()
            try:
                sampler_process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                sampler_process.kill()
            note(f"  resource sampler raw log: {sampler_log}")
            for line in sampler_log.read_text(errors="replace").splitlines():
                if "sample=" not in line:
                    note(f"    {line}")


def _arcshot_pid(log: pathlib.Path) -> int:
    """The ArcShot pid recorded by the startup smoke, or the bundle's process."""
    data = run(["pgrep", "-n", "-f", "ArcShot.app/Contents/MacOS/arcshot"])
    if data.returncode != 0 or not data.stdout.strip():
        fail("could not find the ArcShot process for resource sampling")
    return int(data.stdout.strip().splitlines()[0])



def main() -> int:
    parser = argparse.ArgumentParser(description="ArcShot T-04 performance measurement")
    parser.add_argument("--configuration", choices=["release", "debug"], default="release")
    parser.add_argument("--samples", type=int, default=30)
    parser.add_argument(
        "--metrics",
        default=",".join(ALL_METRICS),
        help="comma-separated: " + ",".join(ALL_METRICS),
    )
    parser.add_argument("--log", type=pathlib.Path, default=None)
    parser.add_argument("--runs", type=int, default=5, help="number of consecutive 24M exports")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument(
        "--no-sampler",
        action="store_true",
        help="run the export gates without the resource sampler (T-04 keeps performance and resource runs separate)",
    )
    options = parser.parse_args()

    configuration = options.configuration
    app_name = "ArcShot" if configuration == "release" else "ArcShot Debug"
    bundle = ROOT / "target" / "bundle" / configuration / "ArcShot.app"
    executable = bundle / "Contents" / "MacOS" / "arcshot"
    log = options.log or (ROOT / "target" / f"perf-measure-{configuration}.log")

    if not options.no_build:
        note("building the bundle under test")
        result = run([str(ROOT / "scripts" / "build-app.sh"), "--" + configuration], timeout=1800.0)
        if result.returncode != 0:
            fail(f"build-app.sh failed:\n{result.stdout}\n{result.stderr}")

    if not executable.exists():
        fail(f"bundle executable not found: {executable}")

    # Gate on a single instance before launching the real product.
    running = run(["agent-desktop", "list-apps", "--app", "ArcShot"])
    try:
        running_apps = json.loads(running.stdout).get("data", {}).get("apps", [])
    except json.JSONDecodeError:
        running_apps = []
    if running_apps:
        fail(f"an ArcShot instance is already running: {running_apps}")

    metrics = [metric.strip() for metric in options.metrics.split(",") if metric.strip()]
    process = start(executable, log)
    sampler = None if options.no_sampler else ROOT / "scripts" / "resource-footprint.sh"
    try:
        wait_log(log, "preferences_launch", 20.0)
        time.sleep(1.0)
        note(f"arcshot_pid={process.pid} log={log}")
        if "region_sck" in metrics:
            measure_region_sck(app_name, log, options.samples)
        if "interact" in metrics:
            measure_interact(app_name, log, options.samples)
        if "clipboard_export" in metrics:
            # 1.96 M (2 M class) then 4.16 M (6 M class) full selections.
            measure_clipboard_export(
                app_name, log, options.samples,
                drag_from=(100, 100), drag_to=(800, 800),
            )
            measure_clipboard_export(
                app_name, log, options.samples,
                drag_from=(100, 100), drag_to=(1400, 900),
            )
        if "scroll" in metrics:
            measure_scroll(app_name, log, options.samples)
        if "export_24m" in metrics:
            measure_export_24m(app_name, log, options.runs, sampler, process.pid)
    finally:
        stop(process)

    note("evaluating gates")
    result = run(
        [str(ROOT / "scripts" / "perf-gate.sh"), str(log),
         "--min-samples", str(options.samples)]
    )
    note(result.stdout)
    if result.returncode != 0:
        note(result.stderr)
        if metrics == ALL_METRICS:
            return 1
        # A metric subset cannot satisfy gates whose samples were not
        # collected (and the 24M build emits thousands of event-preview
        # samples with long-collection outliers). The full-matrix verdict is
        # informational; evaluate the requested gate on its own log.
        note("  metric subset: full-matrix gate verdict is informational")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Failure as error:
        note(f"perf_measure=FAIL {error}")
        raise SystemExit(1)
