#!/usr/bin/env python3
"""Generate or check Cargo notices, rejecting generic license-text fallbacks."""

import argparse
import json
from pathlib import Path
import subprocess

ABOUT_VERSION = "0.9.2"
ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "assets/licenses/cargo-dependencies.txt"


def render(report):
    sections = [
        "# Cargo dependency licenses\n\n"
        f"Generated from Cargo.lock with cargo-about {ABOUT_VERSION} and about.toml.\n"
        "Regenerate with scripts/cargo_licenses.py; do not edit by hand.\n"
        "This inventory includes drawio-png-cli and dependencies across all targets, including\n"
        "build and development dependencies. Not every crate is linked into every\n"
        "binary. License choices apply to the named crates only. See\n"
        "THIRD_PARTY_NOTICES.md for attribution of bundled non-Cargo works."
    ]
    if not report["licenses"]:
        raise ValueError("cargo-about returned no licenses")
    for license in sorted(report["licenses"], key=lambda item: (item["id"], item["text"])):
        crates = sorted(
            (use["crate"] for use in license["used_by"]),
            key=lambda crate: (crate["name"], crate["version"]),
        )
        names = ", ".join(crate["name"] for crate in crates)
        # This upstream revision was verified against the 0.4.0 crate archive.
        # A new release must not silently reuse its pinned external license.
        for crate in crates:
            if crate["name"] == "difflib" and crate["version"] != "0.4.0":
                raise ValueError("review difflib's pinned license source for the new version")
        # --fail alone accepts generic SPDX text without upstream copyright.
        if not license["source_path"] or not license["text"].strip():
            raise ValueError(f"missing upstream license text for {names}; clarify it in about.toml")
        entries = "\n".join(
            f"- {crate['name']} {crate['version']} "
            f"(https://crates.io/crates/{crate['name']}/{crate['version']})"
            for crate in crates
        )
        text = license["text"].replace("\r\n", "\n").rstrip("\n")
        sections.append(f"--- {license['id']} ---\n\nUsed by:\n{entries}\n\n{text}")
    return "\n\n".join(sections) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if committed notices are stale")
    args = parser.parse_args()
    version = subprocess.check_output(["cargo", "about", "--version"], cwd=ROOT, text=True).strip()
    if version != f"cargo-about {ABOUT_VERSION}":
        parser.error(f"expected cargo-about {ABOUT_VERSION}, got {version}")
    report = json.loads(subprocess.check_output(
        ["cargo", "about", "generate", "--locked", "--all-features", "--fail", "--format", "json"],
        cwd=ROOT,
    ))
    try:
        output = render(report)
    except ValueError as error:
        parser.error(str(error))
    if args.check:
        if not OUTPUT.is_file() or OUTPUT.read_bytes() != output.encode("utf-8"):
            parser.error("Cargo notices are stale; run scripts/cargo_licenses.py and review the diff")
        print("Cargo dependency notices are up to date")
    else:
        OUTPUT.write_bytes(output.encode("utf-8"))
        print(f"Generated {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
