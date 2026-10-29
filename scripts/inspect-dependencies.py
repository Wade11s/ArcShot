#!/usr/bin/env python3
"""Print a bounded dependency summary without dumping the full Cargo graph."""

import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def main() -> int:
    if len(sys.argv) > 2 or (len(sys.argv) == 2 and sys.argv[1] != "--show-duplicates"):
        print(f"usage: {sys.argv[0]} [--show-duplicates]", file=sys.stderr)
        return 2
    show_duplicates = len(sys.argv) == 2
    root = Path(__file__).resolve().parent.parent
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        print("dependency inspection failed; run cargo fetch --locked first", file=sys.stderr)
        print(result.stderr, file=sys.stderr, end="")
        return result.returncode

    metadata = json.loads(result.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    members = sorted(metadata["workspace_members"], key=lambda item: packages[item]["name"])

    for member in members:
        direct = []
        for dependency in nodes[member]["deps"]:
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]):
                package = packages[dependency["pkg"]]
                direct.append((package["name"], package["version"]))
        rendered = ", ".join(f"{name} {version}" for name, version in sorted(direct))
        print(f"direct[{packages[member]['name']}]={rendered}")

    sources = sorted({package["source"] or "path" for package in packages.values()})
    unexpected = [source for source in sources if source not in {"path", CRATES_IO}]
    if unexpected:
        print(f"dependency inspection failed: unrecorded sources: {unexpected}", file=sys.stderr)
        return 1
    print(f"dependency_sources={','.join(sources)}")

    versions = defaultdict(set)
    for package in packages.values():
        if package["source"] == CRATES_IO:
            versions[package["name"]].add(package["version"])
    duplicates = {name: sorted(values) for name, values in versions.items() if len(values) > 1}
    print(f"duplicate_package_names={len(duplicates)}")
    if show_duplicates:
        for name, values in sorted(duplicates.items()):
            print(f"duplicate[{name}]={','.join(values)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
