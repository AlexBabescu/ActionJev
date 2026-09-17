#!/usr/bin/env python3
"""Offline tests for the action launcher; no Rust compiler or paid API needed."""
import gzip
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/prepare.sh"
class Installer(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.action = self.root / "trusted action"
        self.dist = self.action / "dist"
        self.dist.mkdir(parents=True)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.exe("uname", '#!/bin/sh\ncase "$1" in -s) echo "${TEST_OS:-Linux}";; -m) echo "${TEST_ARCH:-x86_64}";; esac\n')
        self.exe("cargo", '#!/bin/sh\nprintf "%s\n" "$PWD:$*" > "$CARGO_MARKER"\nmkdir -p "$CARGO_TARGET_DIR/release"\nprintf "#!/bin/sh\nexit 0\n" > "$CARGO_TARGET_DIR/release/actionjev"\nchmod +x "$CARGO_TARGET_DIR/release/actionjev"\n')
        self.output = self.root / "output"
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(("GITHUB_", "GITEA_", "ACTIONJEV_", "TYPESAFE_"))}
        self.env.update(ACTIONJEV_ACTION_PATH=str(self.action), GITHUB_OUTPUT=str(self.output), RUNNER_TEMP=str(self.root), PATH=str(self.bin) + os.pathsep + os.environ["PATH"], CARGO_MARKER=str(self.root / "cargo-called"))
    def exe(self, name, text):
        p = self.bin / name
        p.write_text(text)
        p.chmod(0o700)
        return p
    def bundle(self, target="x86_64-unknown-linux-musl"):
        name = "actionjev-" + target + ".gz"
        data = gzip.compress(b'#!/bin/sh\necho "actionjev test-binary"\n', mtime=0)
        (self.dist / name).write_bytes(data)
        (self.dist / "SHA256SUMS").write_text(hashlib.sha256(data).hexdigest() + "  " + name + "\n")
        return self.dist / name
    def run_prepare(self, expected=0, **env):
        proc = subprocess.run(["bash", str(SCRIPT)], env=self.env | env, cwd=self.root, text=True, capture_output=True)
        self.assertEqual(proc.returncode, expected, proc.stdout + proc.stderr)
        return proc
    def test_bundled_x64(self):
        self.bundle()
        self.run_prepare()
        path = Path(self.output.read_text().strip().split("=", 1)[1])
        self.assertTrue(os.access(path, os.X_OK))
        self.assertFalse((self.root / "cargo-called").exists())
    def test_bundled_arm64(self):
        self.bundle("aarch64-unknown-linux-musl")
        self.run_prepare(TEST_ARCH="aarch64")
    def test_missing_bundle_never_builds(self):
        self.run_prepare(1)
        self.assertFalse((self.root / "cargo-called").exists())
    def test_corrupt_bundle_is_rejected(self):
        path = self.bundle()
        path.write_bytes(b"tampered")
        self.run_prepare(1)
        self.assertFalse(self.output.exists())
    def test_missing_digest_is_rejected(self):
        self.bundle()
        (self.dist / "SHA256SUMS").write_text("")
        self.run_prepare(1)
    def test_duplicate_digest_is_rejected(self):
        self.bundle()
        path = self.dist / "SHA256SUMS"
        path.write_text(path.read_text() * 2)
        self.run_prepare(1)
    def test_unavailable_platform_fails(self):
        self.run_prepare(1, TEST_OS="Darwin")
    def test_preinstalled_binary(self):
        path = self.exe("preinstalled", "#!/bin/sh\nexit 0\n")
        self.run_prepare(ACTIONJEV_BINARY_PATH=str(path))
        self.assertEqual(self.output.read_text(), f"binary={path}\n")
    def test_relative_binary_rejected(self):
        self.run_prepare(1, ACTIONJEV_BINARY_PATH="./untrusted")
    def test_source_build_is_explicit_and_trusted(self):
        self.run_prepare(ACTIONJEV_BUILD_FROM_SOURCE="true")
        self.assertEqual((self.root / "cargo-called").read_text().strip(), f"{self.action}:build --release --locked")
    def test_boolean_validation(self):
        self.run_prepare(1, ACTIONJEV_BUILD_FROM_SOURCE="yes")
    def test_gitea_output(self):
        self.bundle()
        del self.env["GITHUB_OUTPUT"]
        self.run_prepare(GITEA_OUTPUT=str(self.output))
        self.assertTrue(self.output.exists())

if __name__ == "__main__":
    unittest.main(verbosity=2)
