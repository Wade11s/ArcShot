#!/usr/bin/env python3
"""Print a bounded dependency summary without dumping the full Cargo graph."""

import argparse
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def load_metadata(args, root: Path):
    if args.metadata:
        try:
            return json.loads(Path(args.metadata).read_text())
        except (OSError, json.JSONDecodeError) as error:
            print(f"dependency inspection failed: {error}", file=sys.stderr)
            return None

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
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        print(f"dependency inspection failed: invalid Cargo metadata: {error}", file=sys.stderr)
        return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--show-duplicates", action="store_true")
    parser.add_argument("--metadata", help="read a Cargo metadata fixture instead of invoking Cargo")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    metadata = load_metadata(args, root)
    if metadata is None:
        return 1

    try:
        packages = {package["id"]: package for package in metadata["packages"]}
        nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
        members = sorted(metadata["workspace_members"], key=lambda item: packages[item]["name"])
    except (KeyError, TypeError) as error:
        print(f"dependency inspection failed: incomplete metadata: {error}", file=sys.stderr)
        return 1

    for member in members:
        direct = []
        for dependency in nodes[member]["deps"]:
            package = packages[dependency["pkg"]]
            kinds = {entry["kind"] for entry in dependency["dep_kinds"]}
            for kind in kinds:
                prefix = "" if kind is None else f"{kind}: "
                direct.append(f"{prefix}{package['name']} {package['version']}")
        print(f"direct[{packages[member]['name']}]=" + ", ".join(sorted(direct)))

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
    if args.show_duplicates:
        for name, values in sorted(duplicates.items()):
            print(f"duplicate[{name}]={','.join(values)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
