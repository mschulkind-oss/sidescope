"""Unit tests for the Sidescope uvx launcher and doctor."""

from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from sidescope_uvx import format_report, main, run_checks  # noqa: E402


class UvxDoctorTests(unittest.TestCase):
    def test_doctor_succeeds_when_required_runtime_is_present(self) -> None:
        results = run_checks(
            platform_name="linux",
            machine="x86_64",
            env={"WAYLAND_DISPLAY": "wayland-1"},
            dlopen=lambda _lib: True,
            which=lambda name: f"/usr/bin/{name}",
            bundled_bin=lambda: Path("/opt/sidescope"),
        )
        self.assertTrue(all(r.ok for r in results))
        rc = main(
            ["doctor"],
            platform_name="linux",
            machine="x86_64",
            env={"WAYLAND_DISPLAY": "wayland-1"},
            dlopen=lambda _lib: True,
            which=lambda name: f"/usr/bin/{name}",
            bundled_bin=lambda: Path("/opt/sidescope"),
        )
        self.assertEqual(rc, 0)

    def test_missing_gtk4_prints_distro_install_hints_and_blocks_exec(self) -> None:
        called = False

        def fake_execv(_path: str, _args: list[str]) -> None:
            nonlocal called
            called = True

        results = run_checks(
            platform_name="linux",
            machine="x86_64",
            env={},
            dlopen=lambda lib: lib != "libgtk-4.so.1",
            which=lambda _name: None,
            bundled_bin=lambda: Path("/opt/sidescope"),
        )
        report = format_report(results)
        self.assertIn("MISSING  libgtk-4.so.1", report)
        self.assertIn("sudo apt install libgtk-4-1 libadwaita-1-0", report)
        self.assertIn("sudo pacman -S gtk4 libadwaita", report)
        self.assertIn("sudo dnf install gtk4 libadwaita", report)

        rc = main(
            ["--live"],
            execv=fake_execv,
            platform_name="linux",
            machine="x86_64",
            env={},
            dlopen=lambda lib: lib != "libgtk-4.so.1",
            which=lambda _name: None,
            bundled_bin=lambda: Path("/opt/sidescope"),
        )
        self.assertEqual(rc, 1)
        self.assertFalse(called)

    def test_launch_execs_bundled_binary_when_preflight_passes(self) -> None:
        executed: list[tuple[str, list[str]]] = []

        rc = main(
            ["--live"],
            execv=lambda path, args: executed.append((path, args)),
            platform_name="linux",
            machine="x86_64",
            env={"WAYLAND_DISPLAY": "wayland-1"},
            dlopen=lambda _lib: True,
            which=lambda _name: None,
            bundled_bin=lambda: Path("/opt/sidescope"),
        )
        self.assertEqual(rc, 0)
        self.assertEqual(executed, [("/opt/sidescope", ["/opt/sidescope", "--live"])])


if __name__ == "__main__":
    unittest.main()
