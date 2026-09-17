#!/usr/bin/env python3
"""Exercise command authorization and comment updates without GitHub access."""
import importlib.util
import os
import tempfile
import sys
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("review_github", Path(__file__).resolve().parents[1] / "scripts/review_github.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def event():
    return {"action": "created", "repository": {"full_name": helper.REPOSITORY},
            "issue": {"number": 3, "pull_request": {"url": "https://attacker.invalid"}},
            "sender": {"id": 123, "login": "maintainer", "type": "User"},
            "comment": {"body": "/jev review", "user": {"id": 123}}}


PR = {"number": 3, "state": "open", "base": {"ref": "main", "repo": {"full_name": helper.REPOSITORY}}}


class Commands(unittest.TestCase):
    def test_maintainer_dispatches_fixed_workflow_on_main(self):
        with patch.object(helper, "api", side_effect=[{"permission": "write", "role_name": "maintain"}, PR, None]) as api:
            self.assertTrue(helper.dispatch(event()))
        self.assertEqual(api.call_args.args, ("/actions/workflows/jev-review.yml/dispatches", {"ref": "main", "inputs": {"pr-number": "3"}}))

    def test_nonmaintainer_cannot_dispatch(self):
        for role in ("read", "triage", "write", "none"):
            with patch.object(helper, "api", return_value={"permission": role, "role_name": role}) as api:
                self.assertFalse(helper.dispatch(event()))
                self.assertEqual(api.call_count, 1)

    def test_untrusted_event_forms_do_not_contact_api(self):
        examples = []
        for field, value in (("action", "edited"), ("repository", {"full_name": "attacker/repo"}), ("issue", {"number": 3})):
            data = event()
            data[field] = value
            examples.append(data)
        for body in ("/jev review; echo attack", "please /jev review", "/jev review\nrun this", ""):
            data = event()
            data["comment"]["body"] = body
            examples.append(data)
        data = event()
        data["sender"]["id"] = 456
        examples.append(data)
        data = event()
        data["sender"]["type"] = "Bot"
        examples.append(data)
        for data in examples:
            with patch.object(helper, "api") as api:
                self.assertFalse(helper.dispatch(data))
                api.assert_not_called()

    def test_closed_or_wrong_target_pr_cannot_dispatch(self):
        for pr in ({**PR, "state": "closed"}, {**PR, "base": {"ref": "other"}}):
            with patch.object(helper, "api", side_effect=[{"permission": "admin"}, pr]) as api, self.assertRaises(ValueError):
                helper.dispatch(event())
            self.assertEqual(api.call_count, 2)

    def test_approval_uses_fresh_pr_author_not_dispatch_actor(self):
        for author, expected in ((695992, "false"), (123, "true"), (None, "true")):
            with self.subTest(author=author), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "outputs"
                env = {"GITHUB_REPOSITORY": helper.REPOSITORY, "GITHUB_REF": "refs/heads/main",
                       "GITHUB_EVENT_NAME": "workflow_dispatch", "PR_NUMBER": "3",
                       "GITHUB_OUTPUT": str(output), "GITHUB_ACTOR": "AlexBabescu"}
                with patch.dict(os.environ, env, clear=True), patch.object(sys, "argv", ["helper", "request"]), patch.object(helper, "api", return_value={**PR, "user": {"id": author}}), patch.object(helper, "status") as status:
                    helper.main()
                self.assertEqual(output.read_text(), f"approval-required={expected}\n")
                self.assertEqual(status.call_args.kwargs["approval_required"], expected == "true")

    def test_any_base_branch_in_same_repository_is_allowed(self):
        pr = {**PR, "base": {"ref": "feature", "repo": {"full_name": helper.REPOSITORY}}}
        with patch.object(helper, "api", return_value=pr):
            self.assertEqual(helper.validate_pr(3), pr)

    @patch.dict(os.environ, {"GITHUB_RUN_ID": "42"})
    def test_progress_updates_only_bot_comment(self):
        foreign = {"id": 1, "user": {"login": "someone"}, "body": helper.MARKER}
        bot = {"id": 2, "user": {"login": "github-actions[bot]"}, "body": helper.MARKER}
        with patch.object(helper, "api", side_effect=[[foreign, bot], None]) as api:
            helper.status(3)
        self.assertEqual(api.call_args.args[0], "/issues/comments/2")
        self.assertEqual(api.call_args.args[2], "PATCH")
        self.assertIn("/actions/runs/42", api.call_args.args[1]["body"])

    @patch.dict(os.environ, {"GITHUB_RUN_ID": "42"})
    def test_cancelled_old_run_does_not_overwrite_new_run(self):
        bot = {"id": 2, "user": {"login": "github-actions[bot]"}, "body": helper.MARKER + " https://github.com/" + helper.REPOSITORY + "/actions/runs/43"}
        with patch.object(helper, "api", return_value=[bot]) as api:
            helper.status(3, failed=True)
        self.assertEqual(api.call_count, 1)

    @patch.dict(os.environ, {"GITHUB_RUN_ID": "42"})
    def test_failed_run_replaces_its_own_pending_comment(self):
        bot = {"id": 2, "user": {"login": "github-actions[bot]"}, "body": helper.MARKER + " <!-- actionjev:pending:42 -->"}
        with patch.object(helper, "api", side_effect=[[bot], None]) as api:
            helper.status(3, failed=True)
        self.assertIn("Review did not publish a result", api.call_args.args[1]["body"])

    @patch.dict(os.environ, {"GITHUB_RUN_ID": "42"})
    def test_finish_preserves_a_published_report_even_if_job_failed(self):
        bot = {"id": 2, "user": {"login": "github-actions[bot]"}, "body": helper.MARKER + " Review incomplete. [View workflow run](" + helper.run_url() + ")"}
        with patch.object(helper, "api", return_value=[bot]) as api:
            helper.status(3, failed=True)
        self.assertEqual(api.call_count, 1)

    def test_redirects_rejected(self):
        with self.assertRaises(ValueError):
            helper.NoRedirect().redirect_request()


if __name__ == "__main__":
    unittest.main(verbosity=2)
