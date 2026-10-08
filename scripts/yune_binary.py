"""Select one built Yune executable for an entire regression run."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
from typing import Mapping, Sequence


def resolve_binary(
    root: Path,
    *,
    binary: str | None = None,
    profile: str | None = None,
    target_dir: str | None = None,
    target: str | None = None,
    environ: Mapping[str, str] | None = None,
) -> Path:
    env = os.environ if environ is None else environ
    root = root.resolve()
    explicit = binary
    if explicit is None and profile is None and target_dir is None and target is None:
        explicit = env.get("YUNE_BIN")
    if explicit is not None and not explicit.strip():
        raise ValueError("Yune executable path must not be empty")
    selected_profile = profile or env.get("YUNE_PROFILE")

    def rooted(value: str) -> Path:
        path = Path(value).expanduser()
        return (path if path.is_absolute() else root / path).resolve()

    if explicit:
        candidates = [rooted(explicit)]
    else:
        if selected_profile not in (None, "debug", "release"):
            raise ValueError("Yune profile must be debug or release")
        directory = rooted(target_dir or env.get("CARGO_TARGET_DIR") or "target")
        triple = target or env.get("CARGO_BUILD_TARGET")
        if triple:
            if Path(triple).name != triple or triple in (".", "..") or "/" in triple or "\\" in triple:
                raise ValueError("--target must be a target triple, not a path; use --binary for a custom executable")
            directory /= triple
        executable = "yune.exe" if os.name == "nt" else "yune"
        profiles = [selected_profile] if selected_profile else ["debug", "release"]
        candidates = [directory / item / executable for item in profiles]

    existing = [path for path in candidates if path.is_file()]
    if not existing:
        checked = "\n  ".join(str(path) for path in candidates)
        raise FileNotFoundError(
            f"No Yune executable found. Checked:\n  {checked}\n"
            "Build with cargo build -p yune (or --release), then select "
            "--profile debug|release, --binary PATH, or YUNE_BIN."
        )
    if len(existing) != 1:
        raise ValueError(
            "Both debug and release executables exist; choose --profile debug|release "
            "or --binary PATH so tests cannot silently use a stale build."
        )
    chosen = existing[0]
    if os.name != "nt" and not os.access(chosen, os.X_OK):
        raise PermissionError(f"Yune executable is not executable: {chosen}")
    return chosen


def binary_from_args(root: Path, argv: Sequence[str] | None = None) -> Path:
    parser = argparse.ArgumentParser(description="Run Yune regressions against a selected build")
    parser.add_argument("--binary", help="Executable path; relative paths are rooted at the repository (or set YUNE_BIN)")
    parser.add_argument("--profile", choices=("debug", "release"), help="Cargo profile (or set YUNE_PROFILE)")
    parser.add_argument("--target-dir", help="Cargo target directory (or set CARGO_TARGET_DIR)")
    parser.add_argument("--target", help="Cargo target triple subdirectory (or set CARGO_BUILD_TARGET)")
    args = parser.parse_args(argv)
    path = resolve_binary(root, binary=args.binary, profile=args.profile, target_dir=args.target_dir, target=args.target)
    print(f"Yune test executable: {path}", flush=True)
    return path
