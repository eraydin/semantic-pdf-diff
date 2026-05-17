#!/usr/bin/env python3
"""Set all semantic-pdf-diff workspace crate versions consistently."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


WORKSPACE_CRATES = (
    "spdfdiff_types",
    "pdf_core",
    "pdf_content",
    "pdf_text",
    "pdf_semantic",
    "diff_core",
    "diff_report",
    "spdfdiff_cli",
)

ROOT = Path(__file__).resolve().parents[1]
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Update workspace package versions and internal dependency requirements."
    )
    parser.add_argument("version", help="Package version to write, for example 0.1.1")
    parser.add_argument(
        "--internal-dependency-requirement",
        dest="dependency_requirement",
        help=(
            "Version requirement for internal workspace dependencies. "
            "Defaults to the package version. Use =VERSION for preview releases."
        ),
    )
    return parser.parse_args()


def validate_version(version: str) -> None:
    if not SEMVER_RE.fullmatch(version):
        raise SystemExit(f"invalid semver package version: {version}")


def update_package_version(path: Path, version: str) -> None:
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    in_package = False
    updated = False
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped == "[package]":
            in_package = True
            continue
        if in_package and stripped.startswith("["):
            break
        if in_package and line.startswith("version = "):
            lines[index] = f'version = "{version}"\n'
            updated = True
            break
    if not updated:
        raise SystemExit(f"missing [package] version in {path}")
    path.write_text("".join(lines), encoding="utf-8")


def update_workspace_dependency_versions(path: Path, dependency_requirement: str) -> None:
    text = path.read_text(encoding="utf-8")
    for crate in WORKSPACE_CRATES:
        pattern = re.compile(
            rf'^({re.escape(crate)}\s*=\s*\{{\s*version\s*=\s*)"[^"]+"(,\s*path\s*=\s*"crates/{re.escape(crate)}"\s*\}})',
            re.MULTILINE,
        )
        text, replacements = pattern.subn(
            rf'\g<1>"{dependency_requirement}"\2',
            text,
        )
        if crate != "spdfdiff_cli" and replacements != 1:
            raise SystemExit(f"missing workspace dependency version for {crate}")
    path.write_text(text, encoding="utf-8")


def main() -> None:
    args = parse_args()
    version = args.version.strip()
    dependency_requirement = (args.dependency_requirement or version).strip()
    validate_version(version)
    if dependency_requirement.startswith("="):
        validate_version(dependency_requirement[1:])
    else:
        validate_version(dependency_requirement)

    for crate in WORKSPACE_CRATES:
        update_package_version(ROOT / "crates" / crate / "Cargo.toml", version)
    update_workspace_dependency_versions(ROOT / "Cargo.toml", dependency_requirement)


if __name__ == "__main__":
    main()
