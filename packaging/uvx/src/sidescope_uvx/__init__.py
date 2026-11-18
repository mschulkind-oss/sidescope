"""Sidescope uvx launcher and runtime dependency doctor."""

from __future__ import annotations

import ctypes
from dataclasses import dataclass
import os
from pathlib import Path
import platform
import shutil
import sys
from typing import Callable, Mapping, Sequence


REQUIRED_LIBS = (
    ("libgtk-4.so.1", "GTK4 runtime library"),
    ("libadwaita-1.so.0", "libadwaita runtime library"),
)


@dataclass(frozen=True)
class CheckResult:
    """Single runtime check outcome."""

    name: str
    ok: bool
    required: bool
    detail: str


def bundled_binary_path() -> Path | None:
    """Return the bundled sidescope binary inside this package, if present."""
    candidate = Path(__file__).resolve().parent / "bin" / "sidescope"
    if candidate.is_file() and os.access(candidate, os.X_OK):
        return candidate
    return None


def _default_dlopen(lib_name: str) -> bool:
    try:
        ctypes.CDLL(lib_name)
        return True
    except OSError:
        return False


def run_checks(
    *,
    platform_name: str = sys.platform,
    machine: str = platform.machine(),
    env: Mapping[str, str] = os.environ,
    dlopen: Callable[[str], bool] = _default_dlopen,
    which: Callable[[str], str | None] = shutil.which,
    bundled_bin: Callable[[], Path | None] = bundled_binary_path,
) -> list[CheckResult]:
    """Run all runtime readiness checks without executing the native binary."""
    results: list[CheckResult] = []

    is_linux = platform_name.startswith("linux")
    results.append(
        CheckResult(
            name="platform",
            ok=is_linux,
            required=True,
            detail=f"{platform_name} ({machine})"
            if is_linux
            else f"unsupported platform {platform_name}; Sidescope requires Linux + Wayland",
        )
    )

    binary = bundled_bin() or (Path(p) if (p := which("sidescope")) else None)
    results.append(
        CheckResult(
            name="sidescope-binary",
            ok=binary is not None,
            required=True,
            detail=str(binary) if binary else "bundled sidescope binary not found",
        )
    )

    for lib_name, label in REQUIRED_LIBS:
        found = dlopen(lib_name)
        results.append(
            CheckResult(
                name=lib_name,
                ok=found,
                required=True,
                detail=f"{label} found" if found else f"missing {lib_name} ({label})",
            )
        )

    wayland = bool(env.get("WAYLAND_DISPLAY"))
    results.append(
        CheckResult(
            name="WAYLAND_DISPLAY",
            ok=wayland,
            required=False,
            detail=env["WAYLAND_DISPLAY"] if wayland else "unset (needed to open a Wayland window)",
        )
    )

    for tool, purpose in (
        ("ai-usagebar", "required for --live usage cards; sample mode works without it"),
        ("curl", "required for weather and air-quality cards"),
    ):
        path = which(tool)
        results.append(
            CheckResult(
                name=tool,
                ok=path is not None,
                required=False,
                detail=path if path else f"not on PATH ({purpose})",
            )
        )

    return results


def format_report(results: Sequence[CheckResult]) -> str:
    """Format a human-readable doctor report with distro install hints."""
    lines = ["Sidescope runtime doctor:"]
    missing_libs = [r.name for r in results if r.required and not r.ok and r.name.startswith("lib")]

    for r in results:
        if r.ok:
            tag = "ok     "
        elif r.required:
            tag = "MISSING"
        else:
            tag = "warn   "
        lines.append(f"  {tag}  {r.name:<18} {r.detail}")

    if missing_libs:
        lines.extend(
            [
                "",
                "Install the system GTK4 and libadwaita runtime libraries:",
                "  Ubuntu / Debian: sudo apt install libgtk-4-1 libadwaita-1-0",
                "  Arch Linux:      sudo pacman -S gtk4 libadwaita",
                "  Fedora:          sudo dnf install gtk4 libadwaita",
            ]
        )

    return "\n".join(lines)


def main(
    argv: Sequence[str] | None = None,
    *,
    execv: Callable[[str, list[str]], None] = os.execv,
    **check_kwargs,
) -> int:
    """CLI entrypoint for `uvx sidescope` and `uvx sidescope doctor`."""
    args = list(sys.argv[1:] if argv is None else argv)
    results = run_checks(**check_kwargs)

    if args and args[0] in ("doctor", "--doctor"):
        print(format_report(results))
        return 0 if all(r.ok for r in results if r.required) else 1

    missing_required = [r for r in results if r.required and not r.ok]
    if missing_required:
        print(format_report(results), file=sys.stderr)
        return 1

    bundled = check_kwargs.get("bundled_bin", bundled_binary_path)()
    binary = str(bundled) if bundled else check_kwargs.get("which", shutil.which)("sidescope")
    assert binary is not None
    execv(binary, [binary, *args])
    return 0
