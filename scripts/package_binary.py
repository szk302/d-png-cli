#!/usr/bin/env python3
"""Package an already built d-png-cli binary with the redistribution notices."""

import argparse
from pathlib import Path
import tarfile
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("archive", type=Path, help="*.tar.gz, or *.zip for Windows")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    notices = [
        root / "LICENSE",
        root / "THIRD_PARTY_NOTICES.md",
        root / "assets/drawio/README.md",
        root / "assets/drawio/licenses",
        root / "assets/licenses",
    ]
    # Require the generated inventory and the musl notice for the static Linux
    # builds even if a checkout accidentally omits them.
    for path in [
        args.binary,
        root / "assets/licenses/cargo-dependencies.txt",
        root / "assets/licenses/musl-1.2.5-COPYRIGHT.txt",
    ]:
        if not path.is_file():
            parser.error(f"missing distribution input: {path}")
    for path in notices:
        if not path.exists():
            parser.error(f"missing distribution input: {path}")
    if args.archive.resolve() == args.binary.resolve() or any(
        args.archive.resolve() == path.resolve()
        or (path.is_dir() and args.archive.resolve().is_relative_to(path.resolve()))
        for path in notices
    ):
        parser.error("archive must not overwrite a distribution input")
    files = [(args.binary, args.binary.name)]
    for path in notices:
        for item in sorted([path, *path.rglob("*")] if path.is_dir() else [path]):
            if item.is_file():
                files.append((item, item.relative_to(root).as_posix()))
    # Exclusive creation avoids replacing an existing release archive.
    if args.archive.suffix == ".zip":
        with zipfile.ZipFile(args.archive, "x", zipfile.ZIP_DEFLATED) as archive:
            for path, name in files:
                archive.write(path, name)
    else:
        with tarfile.open(args.archive, "x:gz") as archive:
            for path, name in files:
                archive.add(path, arcname=name)
    print(f"Packaged {args.archive} with license notices")


if __name__ == "__main__":
    main()
