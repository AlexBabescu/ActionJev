#!/usr/bin/env python3
"""Offline regression checks for privileged workflow boundaries (PyYAML 6.0.3)."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch, MagicMock

import yaml

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("prepare_pr_review", ROOT / "scripts/prepare-pr-review.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def example():
    return {"number": 17, "state": "open",
            "base": {"repo": {"full_name": helper.REPOSITORY}, "ref": "main", "sha": "a" * 40},
            "head": {"sha": "b" * 40}}


class InputTests(unittest.TestCase):
    def test_positive_number(self):
        self.assertEqual(helper.pr_number("17"), 17)

    def test_invalid_numbers_rejected(self):
        for value in ("", "0", "01", "-1", "1; echo attack", "1\nx=2", "１２", "1.0", "9" * 11):
            with self.subTest(value=value), self.assertRaises(ValueError):
                helper.pr_number(value)

    def test_metadata_accepts_external_head_without_using_its_url(self):
        data = example()
        data["head"]["repo"] = {"clone_url": "https://attacker.invalid/steal", "full_name": "external/fork"}
        data["head"]["ref"] = "$(do-not-execute)"
        self.assertEqual(helper.validate_metadata(data, 17), ("a" * 40, "b" * 40))

    def test_other_base_branches_are_data_only(self):
        data = example()
        data["base"]["ref"] = "feature/$(never-execute)"
        self.assertEqual(helper.validate_metadata(data, 17), ("a" * 40, "b" * 40))

    def test_wrong_repository_rejected(self):
        data = example()
        data["base"]["repo"] = {"full_name": "attacker/fork"}
        with self.assertRaises(ValueError):
            helper.validate_metadata(data, 17)

    def test_wrong_number_or_closed_pr_rejected(self):
        for field, value in (("number", 18), ("state", "closed")):
            data = example()
            data[field] = value
            with self.assertRaises(ValueError):
                helper.validate_metadata(data, 17)

    def test_invalid_sha_rejected(self):
        for value in ("main", "--upload-pack=evil", "a" * 40 + "\nx=1", None):
            data = example()
            data["head"]["sha"] = value
            with self.assertRaises(ValueError):
                helper.validate_metadata(data, 17)

    def test_api_redirect_rejected(self):
        with self.assertRaises(ValueError):
            helper.NoRedirect().redirect_request(None, None, 302, "", {}, "https://attacker.invalid")

    def test_api_uses_fixed_endpoint_and_bounded_read(self):
        opener = MagicMock()
        response = opener.open.return_value.__enter__.return_value
        response.read.return_value = json.dumps(example()).encode()
        with patch.dict(os.environ, {"GH_TOKEN": "test-placeholder"}), patch.object(helper.urllib.request, "build_opener", return_value=opener):
            self.assertEqual(helper.metadata(17)["number"], 17)
        self.assertEqual(opener.open.call_args.args[0].full_url, helper.API + "17")
        response.read.assert_called_once_with(helper.MAX_RESPONSE_BYTES + 1)

    def test_oversized_api_response_rejected(self):
        opener = MagicMock()
        opener.open.return_value.__enter__.return_value.read.return_value = b"x" * (helper.MAX_RESPONSE_BYTES + 1)
        with patch.dict(os.environ, {"GH_TOKEN": "test-placeholder"}), patch.object(helper.urllib.request, "build_opener", return_value=opener), self.assertRaises(ValueError):
            helper.metadata(17)

    def test_non_main_or_non_manual_context_never_contacts_api(self):
        expected = {"GITHUB_REPOSITORY": helper.REPOSITORY, "GITHUB_REF": "refs/heads/main", "GITHUB_EVENT_NAME": "workflow_dispatch"}
        for field, value in (("GITHUB_REF", "refs/pull/17/merge"), ("GITHUB_EVENT_NAME", "pull_request_target"), ("GITHUB_REPOSITORY", "attacker/fork")):
            with patch.dict(os.environ, {**expected, field: value}, clear=True), patch.object(helper, "metadata") as request, self.assertRaises(ValueError):
                helper.main()
            request.assert_not_called()

    def test_preparation_rejects_model_credential(self):
        env = {"GITHUB_REPOSITORY": helper.REPOSITORY, "GITHUB_REF": "refs/heads/main", "GITHUB_EVENT_NAME": "workflow_dispatch", "TYPESAFE_API_KEY": "test-placeholder"}
        with patch.dict(os.environ, env, clear=True), self.assertRaises(ValueError):
            helper.main()

    def test_git_does_not_inherit_credentials_or_overrides(self):
        fake = MagicMock(returncode=0, stdout="ok")
        with patch.dict(os.environ, {"GH_TOKEN": "placeholder", "TYPESAFE_API_KEY": "placeholder", "GIT_CONFIG_COUNT": "1"}), patch.object(helper.subprocess, "run", return_value=fake) as run:
            helper.git(Path("/tmp/repo"), "rev-parse", "HEAD")
        env = run.call_args.kwargs["env"]
        self.assertNotIn("GH_TOKEN", env)
        self.assertNotIn("TYPESAFE_API_KEY", env)
        self.assertNotIn("GIT_CONFIG_COUNT", env)
        self.assertEqual(env["GIT_CONFIG_GLOBAL"], os.devnull)
        self.assertFalse(run.call_args.kwargs.get("shell", False))


class GitTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.run_git("init", "-q")
        self.run_git("config", "user.name", "Offline test")
        self.run_git("config", "user.email", "test@example.invalid")
        (self.source / "app.py").write_text("print('base')\n")
        self.run_git("add", ".")
        self.run_git("commit", "-qm", "base")
        self.base = self.run_git("rev-parse", "HEAD")
        (self.source / "app.py").write_text("raise RuntimeError('must not execute')\n")
        (self.source / "action.yml").write_text("untrusted action that must not be loaded\n")
        (self.source / "secret-link").symlink_to("/etc/passwd")
        self.run_git("add", ".")
        self.run_git("commit", "-qm", "untrusted PR")
        self.head = self.run_git("rev-parse", "HEAD")
        self.run_git("update-ref", "refs/pull/17/head", self.head)
        self.data = example()
        self.data["base"]["sha"] = self.base
        self.data["head"]["sha"] = self.head
        self.real_git = helper.git

    def run_git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.source), *args], stderr=subprocess.DEVNULL, text=True).strip()

    def local_fetch(self, repo, *args):
        # Only tests replace the fixed public origin with a local fixture.
        if args[0] == "fetch":
            self.assertIn(helper.REMOTE, args)
            args = ("-c", "protocol.file.allow=always", *(str(self.source) if a == helper.REMOTE else a for a in args))
        return self.real_git(repo, *args)

    def test_prepare_fetches_objects_without_checking_out_pr_files(self):
        with patch.object(helper, "git", side_effect=self.local_fetch):
            result = helper.prepare(self.data, 17, self.root)
        repo = Path(result["path"])
        self.assertEqual(self.real_git(repo, "rev-parse", "--is-bare-repository"), "true")
        self.assertFalse((repo / "app.py").exists())
        self.assertFalse((repo / "action.yml").exists())
        self.assertFalse((repo / "secret-link").exists())
        self.assertEqual(result["head"], self.head)
        self.assertEqual(self.real_git(repo, "merge-base", result["base"], result["head"]), self.base)
        self.assertIn("RuntimeError", self.real_git(repo, "show", self.head + ":app.py"))

    def test_stale_head_fails_before_review(self):
        self.data["head"]["sha"] = "c" * 40
        with patch.object(helper, "git", side_effect=self.local_fetch), self.assertRaisesRegex(ValueError, "head changed"):
            helper.prepare(self.data, 17, self.root)


class WorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.review = yaml.safe_load((ROOT / ".github/workflows/jev-review.yml").read_text())
        cls.release = yaml.safe_load((ROOT / ".github/workflows/main.yml").read_text())
        cls.auto = yaml.safe_load((ROOT / ".github/workflows/jev-auto.yml").read_text())
        cls.ci = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())

    def test_review_is_manual_only(self):
        # YAML 1.1 loaders interpret 'on' as True; GitHub uses YAML 1.2.
        trigger = self.review.get("on", self.review.get(True))
        self.assertEqual(set(trigger), {"workflow_dispatch"})
        self.assertIn("#${{ inputs.pr-number }}", self.review["run-name"])

    def test_review_requires_main_and_environment(self):
        job = self.review["jobs"]["review"]
        self.assertIn("github.ref == 'refs/heads/main'", job["if"])
        self.assertIn("github.repository == 'AlexBabescu/ActionJev'", job["if"])
        self.assertEqual(job["environment"], "jev-api")
        self.assertEqual(job["runs-on"], "ubuntu-24.04")
        self.assertEqual(job["permissions"], {"contents": "read", "pull-requests": "write"})

    def test_review_pins_action_and_never_loads_pr_checkout(self):
        steps = self.review["jobs"]["review"]["steps"]
        self.assertEqual(len(steps), 6)
        self.assertEqual(steps[0]["with"]["ref"], "${{ github.sha }}")
        self.assertFalse(steps[0]["with"]["persist-credentials"])
        self.assertEqual(steps[1]["run"], "python3 trusted/scripts/prepare-pr-review.py")
        self.assertNotIn("secrets.", json.dumps(steps[:4]))
        self.assertEqual(steps[3]["working-directory"], "trusted")
        self.assertEqual(steps[3]["run"], "cargo build --release --locked")
        self.assertEqual(steps[4]["if"], "inputs.calibrate")
        self.assertEqual(steps[5]["uses"], "./trusted")
        inputs = steps[5]["with"]
        self.assertNotIn("build-from-source", inputs)
        self.assertEqual(inputs["policy"], "${{ github.workspace }}/trusted/prompts/review.json")
        self.assertEqual(inputs["binary-path"], "${{ github.workspace }}/trusted/target/release/actionjev")
        self.assertEqual(inputs["typesafe-url"], "https://api.typesafe.ai/v1/systemone")
        self.assertEqual(inputs["api-url"], "https://api.github.com")
        self.assertEqual(inputs["path"], "${{ steps.pr.outputs.path }}")

    def test_command_and_status_jobs_never_receive_model_secrets(self):
        command = yaml.safe_load((ROOT / ".github/workflows/jev-command.yml").read_text())
        self.assertEqual(command.get("on", command.get(True)), {"issue_comment": {"types": ["created"]}})
        self.assertNotIn("secrets.", json.dumps(command))
        self.assertNotIn("environment", command["jobs"]["request"])
        self.assertEqual(command["jobs"]["request"]["permissions"], {"contents": "read", "actions": "write"})
        for name in ("request", "finish"):
            job = self.review["jobs"][name]
            self.assertNotIn("secrets.", json.dumps(job))
            self.assertNotIn("environment", job)
            self.assertEqual(job["steps"][0]["with"]["ref"], "${{ github.sha }}")

    def test_ordinary_ci_does_not_reference_secrets(self):
        for name in ("workflow-security", "package"):
            job = self.ci["jobs"][name]
            self.assertNotIn("secrets.", json.dumps(job))
            self.assertNotIn("environment", job)
        self.assertEqual(self.ci["permissions"], {"contents": "read"})

    def test_auto_dispatch_has_no_checkout_or_model_access(self):
        triggers = self.auto.get("on", self.auto.get(True))
        self.assertEqual(set(triggers), {"pull_request_target"})
        self.assertEqual(set(triggers["pull_request_target"]["types"]),
                         {"opened", "synchronize", "reopened", "ready_for_review", "edited"})
        self.assertEqual(self.auto["permissions"], {})
        job = self.auto["jobs"]["request"]
        self.assertEqual(job["permissions"], {"actions": "write"})
        self.assertNotIn("environment", job)
        self.assertNotIn("secrets.", json.dumps(job))
        self.assertEqual(len(job["steps"]), 1)
        step = job["steps"][0]
        self.assertNotIn("uses", step)
        self.assertNotIn("${{", step["run"])
        self.assertIn('--repo AlexBabescu/ActionJev --ref main', step["run"])
        self.assertIn('^\u005b1-9][0-9]{0,9}$', step["run"])

    def test_dispatch_command_rejects_shell_injection(self):
        command = self.auto["jobs"]["request"]["steps"][0]["run"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake = root / "gh"
            fake.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$CAPTURE"\n')
            fake.chmod(0o755)
            capture = root / "args"
            env = {"PATH": f"{root}:{os.defpath}", "CAPTURE": str(capture)}
            result = subprocess.run(["bash", "-e", "-c", command], env={**env, "PR_NUMBER": "17"}, capture_output=True)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(capture.read_text().splitlines(), ["workflow", "run", "jev-review.yml", "--repo", "AlexBabescu/ActionJev", "--ref", "main", "-f", "pr-number=17"])
            capture.unlink()
            for number in ("", "0", "1; touch bad", "$(touch bad)", "1\nx=2"):
                result = subprocess.run(["bash", "-e", "-c", command], env={**env, "PR_NUMBER": number}, capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(capture.exists())

    def test_external_approval_cannot_be_skipped_by_review(self):
        approval = self.review["jobs"]["approve"]
        self.assertEqual(approval["environment"], "jev-approval")
        self.assertEqual(approval["if"], "needs.request.outputs.approval-required != 'false'")
        job = self.review["jobs"]["review"]
        self.assertEqual(job["needs"], ["request", "approve"])
        self.assertIn("needs.request.result == 'success'", job["if"])
        self.assertIn("needs.approve.result == 'success'", job["if"])
        self.assertIn("needs.approve.result == 'skipped' && needs.request.outputs.approval-required == 'false'", job["if"])
        self.assertIn("!cancelled()", job["if"])

    def test_release_jobs_are_absent_from_pr_checks(self):
        self.assertEqual(set(self.ci["jobs"]), {"workflow-security", "package"})
        self.assertEqual(set(self.ci.get("on", self.ci.get(True))), {"pull_request", "workflow_call"})
        self.assertEqual(set(self.release.get("on", self.release.get(True))), {"push", "workflow_dispatch"})
        self.assertEqual(self.release["jobs"]["checks"]["uses"], "./.github/workflows/ci.yml")
        self.assertEqual(self.release["jobs"]["approve"]["environment"], "jev-approval")
        self.assertIn("approve", self.release["jobs"]["live"]["needs"])
        self.assertIn("checks", self.release["jobs"]["publish"]["needs"])

    def test_live_and_publish_are_main_only_not_pr_or_old_feature_branch(self):
        for name in ("live", "publish", "consume"):
            guard = self.release["jobs"][name]["if"]
            self.assertIn("github.ref == 'refs/heads/main'", guard)
            self.assertIn("github.event_name == 'push'", guard)
            self.assertIn("github.event_name == 'workflow_dispatch'", guard)
            self.assertNotIn("feat/", guard)
        self.assertEqual(self.release["jobs"]["live"]["environment"], "jev-api")
        self.assertIn("live", self.release["jobs"]["publish"]["needs"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
