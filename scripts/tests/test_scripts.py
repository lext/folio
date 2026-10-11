"""Repository tooling regressions; no network, TeX, GPU or Rust build required."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tarfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CheckScriptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="folio scripts ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "commands"
        cargo = self.bin / "cargo"
        cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$FOLIO_COMMAND_LOG"\nexit "${FOLIO_CARGO_EXIT:-0}"\n')
        cargo.chmod(0o755)
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                        FOLIO_COMMAND_LOG=str(self.log))

    def run_check(self, *args):
        result = subprocess.run([str(ROOT / "scripts/check.sh"), *args],
                                env=self.env, text=True, capture_output=True)
        commands = self.log.read_text().splitlines() if self.log.exists() else []
        return result, commands

    def test_default_checks_need_no_tex_or_pdfium(self):
        result, commands = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(c.startswith("test ") for c in commands))
        self.assertFalse(any("--ignored" in c for c in commands), commands)
        self.assertFalse(any("desktop/Cargo.toml" in c for c in commands), commands)
        for command in commands:
            if not command.startswith("fmt "):
                self.assertIn("--offline", command)
                self.assertIn("--locked", command)

    def test_desktop_unit_checks_do_not_require_tex(self):
        result, commands = self.run_check("--desktop")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any("desktop/Cargo.toml" in c for c in commands))
        self.assertFalse(any("--ignored" in c or "folio-cli" in c for c in commands))

    def test_tex_checks_are_explicit_and_skip_manual_profiles(self):
        result, commands = self.run_check("--desktop", "--tex")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any("--test tex_integration" in c and "--ignored" in c for c in commands))
        self.assertTrue(any("desktop/Cargo.toml" in c and "--ignored" in c
                            and "--skip profile_" in c for c in commands))
        build = next(c for c in commands if "folio-cli" in c)
        self.assertNotIn("build examples/welcome", build, "checks must use a disposable project")

    def test_help_and_bad_arguments_do_not_start_builds(self):
        result, commands = self.run_check("--help")
        self.assertEqual(result.returncode, 0)
        self.assertIn("--tex", result.stdout)
        self.assertEqual(commands, [])
        result, commands = self.run_check("--desktpo")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--desktpo", result.stderr)
        self.assertEqual(commands, [])

    def test_build_failures_stop_the_script(self):
        self.env["FOLIO_CARGO_EXIT"] = "42"
        result, commands = self.run_check("--desktop", "--tex")
        self.assertEqual(result.returncode, 42)
        self.assertEqual(len(commands), 1)


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="folio package ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory in ["bin", "scripts", "desktop/target/release", "runtime/lib",
                          "runtime/licenses", "vendor/block"]:
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / "scripts/package.sh", self.root / "scripts/package.sh")
        (self.root / "desktop/Cargo.toml").write_text('[package]\nname = "folio"\nversion = "9.8.7"\n')
        for filename in ["LICENSE", "THIRD_PARTY.md", "runtime/LICENSE", "runtime/licenses/pdfium.txt",
                         "vendor/block/LICENSE", "folio.toml", "runtime/lib/libpdfium.so"]:
            (self.root / filename).write_text(filename)
        self.log = self.root / "commands"
        for filename, content in {
            "bin/cargo": '#!/bin/sh\nprintf "%s\\n" "$*" >> "$FOLIO_COMMAND_LOG"\n',
            "bin/uname": '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; esac\n',
            "desktop/target/release/folio": '#!/bin/sh\nexit 0\n',
        }.items():
            path = self.root / filename
            path.write_text(content)
            path.chmod(0o755)
        self.env = dict(os.environ, PATH=f"{self.root / 'bin'}:{os.environ['PATH']}",
                        FOLIO_COMMAND_LOG=str(self.log))

    def package(self):
        return subprocess.run(["bash", str(self.root / "scripts/package.sh")],
                              env=self.env, text=True, capture_output=True)

    def test_archive_preserves_executable_settings_and_notices(self):
        bundle = self.root / "dist/folio-linux-x86_64"
        bundle.mkdir(parents=True)
        (bundle / "folio.toml").write_text("# user's settings\n")
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = self.root / "dist/folio-9.8.7-linux-x86_64.tar.gz"
        self.assertTrue(archive.is_file(), result.stdout)
        with tarfile.open(archive) as packed:
            self.assertTrue(packed.getmember("folio-linux-x86_64/folio").mode & 0o111)
            self.assertEqual(packed.extractfile("folio-linux-x86_64/folio.toml").read(),
                             b"# user's settings\n")
            self.assertTrue(packed.getmember("folio-linux-x86_64/licenses/PDFium-LICENSE").isfile())
            self.assertTrue(packed.getmember("folio-linux-x86_64/licenses/Folio-LICENSE").isfile())

    def test_missing_runtime_fails_before_expensive_build(self):
        (self.root / "runtime/lib/libpdfium.so").unlink()
        result = self.package()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("setup-pdfium.sh", result.stderr)
        self.assertFalse(self.log.exists())


class IgnoreRulesTests(unittest.TestCase):
    def test_generated_files_are_ignored_but_pdf_assets_and_lockfiles_are_not(self):
        with tempfile.TemporaryDirectory(prefix="folio ignore ") as folder:
            subprocess.run(["git", "init", "--quiet", folder], check=True)
            shutil.copyfile(ROOT / ".gitignore", Path(folder) / ".gitignore")
            ignored = ["target/debug/folio-cli", "desktop/target/release/folio",
                       ".cargo-local/config.toml", "runtime/lib/libpdfium.so",
                       "dist/Folio.app/Contents/MacOS/folio", "examples/Test/main.tex",
                       "examples/welcome/.folio/main.pdf", "notes.aux", ".env",
                       "scripts/tests/__pycache__/test_scripts.pyc"]
            included = ["Cargo.lock", "desktop/Cargo.lock", "folio.toml",
                        "examples/welcome/figures/diagram.pdf", "docs/images/preview.png",
                        "vendor/block/LICENSE", "vendor/block/Cargo.lock"]
            for path in ignored + included:
                result = subprocess.run(["git", "-C", folder, "check-ignore", "--quiet", path])
                self.assertEqual(result.returncode, 0 if path in ignored else 1, path)


if __name__ == "__main__":
    unittest.main()
