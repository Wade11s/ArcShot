#!/usr/bin/env python3
"""Exercise dependency inspection against deterministic metadata fixtures."""

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
ROOT = Path(__file__).resolve().parent.parent
INSPECTOR = ROOT / "scripts" / "inspect-dependencies.py"


def package(identifier, name, version, source=CRATES_IO):
    return {"id": identifier, "name": name, "version": version, "source": source}


def dependency(identifier, kind):
    return {"name": identifier, "pkg": identifier, "dep_kinds": [{"kind": kind}]}


def metadata_fixture():
    return {
        "packages": [
            package("app", "arcshot-app", "0.1.0", None),
            package("core", "arcshot-core", "0.1.0", None),
            package("build", "build-tool", "1.0.0"),
            package("dev", "dev-tool", "1.0.0"),
            package("shared-1", "shared", "1.0.0"),
            package("shared-2", "shared", "2.0.0"),
        ],
        "workspace_members": ["app", "core"],
        "resolve": {
            "nodes": [
                {
                    "id": "app",
                    "deps": [
                        dependency("core", None),
                        dependency("build", "build"),
                        dependency("dev", "dev"),
                    ],
                },
                {"id": "core", "deps": []},
            ]
        },
    }


def run(*arguments):
    return subprocess.run(
        [sys.executable, str(INSPECTOR), *arguments],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="arcshot-dependencies-") as directory:
        directory = Path(directory)
        valid = directory / "valid.json"
        valid.write_text(json.dumps(metadata_fixture()))
        result = run("--metadata", str(valid))
        if result.returncode != 0:
            print(f"valid dependency fixture failed: {result.stderr}", file=sys.stderr)
            return 1
        for expected in (
            "arcshot-core 0.1.0",
            "build: build-tool 1.0.0",
            "dev: dev-tool 1.0.0",
            "duplicate_package_names=1",
            f"dependency_sources=path,{CRATES_IO}",
        ):
            if expected not in result.stdout:
                print(f"missing {expected!r} in {result.stdout}", file=sys.stderr)
                return 1

        result = run("--metadata", str(valid), "--show-duplicates")
        if result.returncode != 0 or "duplicate[shared]=1.0.0,2.0.0" not in result.stdout:
            print(f"duplicate expansion failed: {result.stdout}{result.stderr}", file=sys.stderr)
            return 1

        invalid_source = metadata_fixture()
        invalid_source["packages"].append(package("git", "unrecorded", "1.0.0", "git+https://example.invalid/repo"))
        source_path = directory / "invalid-source.json"
        source_path.write_text(json.dumps(invalid_source))
        result = run("--metadata", str(source_path))
        if result.returncode == 0 or "unrecorded sources" not in result.stderr:
            print("unexpected dependency source was accepted", file=sys.stderr)
            return 1

        malformed = directory / "malformed.json"
        malformed.write_text("not JSON")
        result = run("--metadata", str(malformed))
        if result.returncode == 0:
            print("malformed metadata was accepted", file=sys.stderr)
            return 1

        result = run("--metadata", str(directory / "missing.json"))
        if result.returncode == 0:
            print("missing metadata was accepted", file=sys.stderr)
            return 1

    print("dependency_inspection=pass")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
