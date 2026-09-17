#!/usr/bin/env python3
"""Offline HTTP-contract and Git integration tests. No real API key is used."""
import contextlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit

BINARY = str(Path(sys.argv.pop(1)).resolve()) if len(sys.argv) > 1 else str(Path("target/release/actionjev").resolve())

class Fixture:
    def __init__(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name)
        self.git("init", "-b", "main")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Contract Test")
        (self.path / "lib.rs").write_text("pub fn allowed(admin: bool) -> bool { admin }\n")
        self.commit()
        self.base = self.git("rev-parse", "HEAD").strip()
        (self.path / "lib.rs").write_text("pub fn allowed(admin: bool) -> bool { true }\n")
        self.commit()
        self.head = self.git("rev-parse", "HEAD").strip()
    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.path), *args], stderr=subprocess.DEVNULL, text=True)
    def commit(self):
        self.git("add", "--all")
        self.git("commit", "-qm", "fixture")
    def close(self):
        self.tmp.cleanup()

class Service:
    def __init__(self):
        self.requests = []
        self.posts = []
        self.patches = []
        self.pages = []
        self.confidence = 0.95
        self.screen_all = False
        self.screen_none = False
        self.none_evidence = False
        self.malformed = False
        self.statuses = []
        self.head = ""
        self.comments = []
        self.lock = threading.Lock()
    def evaluate(self, body):
        result = {}
        for key, q in body["questions"].items():
            if q["type"] == "noul":
                value = 0.05 if self.screen_none else (0.95 if key in ("correctness", "supported") or self.screen_all else 0.05)
                result[key] = {"type": "noul", "noul": value}
            elif q["type"] == "choice":
                choices = q["criteria"]
                selected = "incorrect_condition" if "incorrect_condition" in choices else next(k for k in choices if k != "none")
                if self.none_evidence and key == "evidence":
                    selected = "none"
                result[key] = {"type": "choice", "choice": selected, "probabilities": {k: float(k == selected) for k in choices}, "confidence": self.confidence}
            elif q["type"] == "score":
                result[key] = {"type": "score", "score": 3.0, "probabilities": {str(i): float(i == 3) for i in range(len(q["criteria"]))}, "confidence": self.confidence}
        return {"model": "jev-contract-test", "answers": result, "usage": {"input_tokens": 100, "output_tokens": 10}}

@contextlib.contextmanager
def server(service):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def reply(self, value, status=200):
            data = json.dumps(value).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Retry-After", "0")
            self.end_headers()
            self.wfile.write(data)
        def body(self):
            return json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
        def do_POST(self):
            data = self.body()
            if self.path == "/v1/systemone":
                with service.lock:
                    service.requests.append(data)
                    status = service.statuses.pop(0) if service.statuses else 200
                if status != 200:
                    return self.reply({"message": "secret-upstream-error-body"}, status)
                if service.malformed:
                    return self.reply({"unexpected": "response"})
                self.reply(service.evaluate(data))
            else:
                service.posts.append((self.path, data))
                self.reply({"id": 43})
        def do_PATCH(self):
            service.patches.append((self.path, self.body()))
            self.reply({"id": 42})
        def do_GET(self):
            request = urlsplit(self.path)
            path = request.path
            if path.endswith("/pulls/7"):
                self.reply({"head": {"sha": service.head}})
            elif path.endswith("/user"):
                self.reply({"login": "review-bot"})
            elif path.endswith("/comments"):
                query = parse_qs(request.query)
                page = int(query.get("page", ["1"])[0])
                size = int(query.get("limit", query.get("per_page", ["50"]))[0])
                service.pages.append(query)
                self.reply(service.comments[(page - 1) * size:page * size])
            else:
                self.reply({"error": "unexpected route"}, 404)
    http = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    worker = threading.Thread(target=http.serve_forever, daemon=True)
    worker.start()
    try:
        yield f"http://127.0.0.1:{http.server_port}"
    finally:
        http.shutdown()
        http.server_close()
        worker.join()

class Contracts(unittest.TestCase):
    def setUp(self):
        self.repo = Fixture()
        self.addCleanup(self.repo.close)
    def run_review(self, service=None, extra=(), mode="changes", base=None, expected=0, env_extra=None):
        service = service or Service()
        with tempfile.TemporaryDirectory() as output, server(service) as url:
            env = {k: v for k, v in os.environ.items() if not k.startswith(("GITHUB_", "GITEA_", "ACTIONJEV_", "TYPESAFE_"))}
            env["TYPESAFE_API_KEY"] = "test-key-not-a-secret"
            env["ACTIONJEV_TOKEN"] = "test-bot-not-a-secret"
            env.update(env_extra or {})
            command = [BINARY, "--repo", str(self.repo.path), "--mode", mode, "--head", "HEAD", "--platform", "none", "--output-dir", output, "--typesafe-url", url + "/v1/systemone", "--allow-insecure-http", "--concurrency", "2"]
            if mode == "changes":
                command.extend(["--base", base or self.repo.base])
            # Tests can deliberately replace singleton arguments.
            for key, value in extra:
                if key in command:
                    i = command.index(key)
                    del command[i:i + 2]
                command.append(key)
                if value is not None:
                    command.append(value.replace("{url}", url))
            proc = subprocess.run(command, env=env, text=True, capture_output=True, timeout=30)
            self.assertEqual(proc.returncode, expected, proc.stdout + proc.stderr)
            report = json.loads((Path(output) / "report.json").read_text()) if (Path(output) / "report.json").exists() else None
            traces = [json.loads(line) for line in (Path(output) / "trace.jsonl").read_text().splitlines()] if (Path(output) / "trace.jsonl").exists() else []
            self.assertNotIn("test-key-not-a-secret", proc.stdout + proc.stderr)
            self.assertNotIn("test-bot-not-a-secret", proc.stdout + proc.stderr)
            return report, traces, proc
    def test_pipeline_and_trace(self):
        service = Service()
        report, traces, _ = self.run_review(service)
        self.assertEqual(report["api_calls"], 3)
        self.assertEqual(report["scanned_files"], 1)
        self.assertEqual(report["input_tokens"], 300)
        self.assertTrue(report["findings"][0]["actionable"])
        self.assertEqual(len(traces), 3)
        self.assertNotIn("source_at_reviewed_commit", json.dumps(traces))
        self.assertEqual(service.requests[0]["model"], "jev-latest")
    def test_gate(self):
        self.run_review(extra=[("--fail-on", "high")], expected=3)
    def test_uncertainty_does_not_gate(self):
        service = Service()
        service.confidence = 0.1
        report, _, _ = self.run_review(service, extra=[("--fail-on", "low")])
        self.assertFalse(report["findings"][0]["actionable"])
    def test_no_evidence_no_finding(self):
        service = Service()
        service.none_evidence = True
        report, _, _ = self.run_review(service)
        self.assertEqual(report["findings"], [])
        self.assertEqual(report["api_calls"], 2)
    def test_no_screen_signals(self):
        service = Service()
        service.screen_none = True
        report, _, _ = self.run_review(service)
        self.assertEqual(report["findings"], [])
        self.assertEqual(report["api_calls"], 1)
    def test_screened_out_comment_explains_where_review_stopped(self):
        service = Service()
        service.screen_none = True
        service.head = self.repo.head
        self.run_review(service, extra=[("--comment", None), ("--platform", "github"), ("--repository", "a/b"), ("--pr-number", "7"), ("--api-url", "{url}/api/v3")])
        body = service.posts[0][1]["body"]
        self.assertIn("No potential issues passed screening", body)
        self.assertIn("0.650", body)
        self.assertNotIn("survived evidence selection", body)
        self.assertIn("<summary>Screening scores and API usage</summary>", body)
    def test_dry_run_no_calls(self):
        report, _, _ = self.run_review(extra=[("--dry-run", None)])
        self.assertTrue(report["dry_run"])
        self.assertEqual(report["api_calls"], 0)
    def test_api_failure_is_not_clean_review(self):
        service = Service()
        service.malformed = True
        report, _, _ = self.run_review(service, expected=1)
        self.assertIsNone(report)
    def test_overload_retries(self):
        service = Service()
        service.statuses = [529]
        report, _, _ = self.run_review(service)
        self.assertEqual(report["http_attempts"], 4)
    def test_auth_error_not_retried_or_logged(self):
        service = Service()
        service.statuses = [401]
        _, _, proc = self.run_review(service, expected=1)
        self.assertEqual(len(service.requests), 1)
        self.assertNotIn("secret-upstream-error-body", proc.stderr)
    def test_followup_budget_is_explicit(self):
        service = Service()
        service.screen_all = True
        report, _, _ = self.run_review(service, extra=[("--max-followups", "1")], expected=2)
        self.assertFalse(report["complete"])
    def test_allow_incomplete(self):
        service = Service()
        service.screen_all = True
        report, _, _ = self.run_review(service, extra=[("--max-followups", "1"), ("--allow-incomplete", None)])
        self.assertFalse(report["complete"])
    def test_secret_and_symlink_exclusion(self):
        (self.repo.path / ".env").write_text("NEVER_SEND=this-is-private\n")
        (self.repo.path / "credentials.json").write_text('{"NEVER_SEND":true}')
        (self.repo.path / "linked.rs").symlink_to(".env")
        self.repo.commit()
        service = Service()
        report, _, _ = self.run_review(service)
        self.assertEqual(report["scanned_files"], 1)
        self.assertNotIn("NEVER_SEND", json.dumps(service.requests))
        self.assertTrue(any(s["reason"] == "symlink_or_submodule" for s in report["skipped"]))
    def test_working_tree_and_untracked_code_ignored(self):
        (self.repo.path / "lib.rs").write_text("NEVER_READ_WORKTREE\n")
        (self.repo.path / "extra.rs").write_text("NEVER_READ_WORKTREE\n")
        service = Service()
        self.run_review(service)
        self.assertNotIn("NEVER_READ_WORKTREE", json.dumps(service.requests))
    def test_literal_paths(self):
        (self.repo.path / "[strange] name.rs").write_text("fn example() {}\n")
        self.repo.commit()
        report, _, _ = self.run_review()
        self.assertEqual(report["scanned_files"], 2)
    def test_full_codebase(self):
        report, _, _ = self.run_review(mode="codebase")
        self.assertEqual(report["mode"], "codebase")
        self.assertIsNone(report["merge_base"])
        self.assertEqual(report["scanned_files"], 1)
    def test_deleted_file_old_side(self):
        base = self.repo.git("rev-parse", "HEAD").strip()
        (self.repo.path / "lib.rs").unlink()
        self.repo.commit()
        report, _, _ = self.run_review(base=base)
        self.assertEqual(report["findings"][0]["evidence"]["new_lines"], 0)
    def test_merge_base_not_tip_to_tip(self):
        original = self.repo.base
        topic = self.repo.head
        self.repo.git("checkout", "-qb", "target", original)
        (self.repo.path / "unrelated.rs").write_text("fn unrelated() {}\n")
        self.repo.commit()
        target = self.repo.git("rev-parse", "HEAD").strip()
        self.repo.git("checkout", "--detach", topic)
        report, _, _ = self.run_review(base=target)
        self.assertEqual(report["merge_base"], original)
        self.assertEqual(report["scanned_files"], 1)
    def test_file_budget(self):
        (self.repo.path / "second.rs").write_text("fn second() {}\n")
        self.repo.commit()
        report, _, _ = self.run_review(extra=[("--max-files", "1")], expected=2)
        self.assertFalse(report["complete"])
        self.assertTrue(any(s["reason"] == "max_files" for s in report["skipped"]))
    def test_gitea_comment_create_and_update(self):
        service = Service()
        service.head = self.repo.head
        options = [("--comment", None), ("--platform", "gitea"), ("--repository", "a/b"), ("--pr-number", "7"), ("--api-url", "{url}/gitea/api/v1")]
        self.run_review(service, extra=options)
        self.assertEqual(len(service.posts), 1)
        self.assertEqual(service.posts[0][0], "/gitea/api/v1/repos/a/b/issues/7/comments")
        self.assertIn("limit", service.pages[0])
        service.posts.clear()
        service.comments = [{"id": 42, "user": {"login": "review-bot"}, "body": "<!-- actionjev:review:v1 --> old"}]
        self.run_review(service, extra=options)
        self.assertEqual(len(service.patches), 1)
        self.assertEqual(len(service.posts), 0)
    def test_github_comment_endpoint(self):
        service = Service()
        service.head = self.repo.head
        self.run_review(service, extra=[("--comment", None), ("--platform", "github"), ("--repository", "a/b"), ("--pr-number", "7"), ("--api-url", "{url}/api/v3")], env_extra={"GITHUB_SERVER_URL": "https://github.com", "GITHUB_REPOSITORY": "a/b", "GITHUB_RUN_ID": "42"})
        self.assertEqual(service.posts[0][0], "/api/v3/repos/a/b/issues/7/comments")
        self.assertIn("per_page", service.pages[0])
        self.assertIn("[View workflow run](https://github.com/a/b/actions/runs/42)", service.posts[0][1]["body"])
    def test_stale_review_not_published(self):
        service = Service()
        service.head = "0" * 40
        self.run_review(service, extra=[("--comment", None), ("--platform", "github"), ("--repository", "a/b"), ("--pr-number", "7"), ("--api-url", "{url}/api/v3")])
        self.assertEqual(service.posts, [])
        self.assertEqual(service.patches, [])
    def test_foreign_marker_not_overwritten(self):
        service = Service()
        service.head = self.repo.head
        service.comments = [{"id": 42, "user": {"login": "someone-else"}, "body": "<!-- actionjev:review:v1 -->"}]
        self.run_review(service, extra=[("--comment", None), ("--platform", "gitea"), ("--repository", "a/b"), ("--pr-number", "7"), ("--api-url", "{url}/api/v1")])
        self.assertEqual(len(service.posts), 1)
        self.assertEqual(service.patches, [])
    def test_comment_on_second_page_is_updated(self):
        service = Service()
        service.head = self.repo.head
        service.comments = [{"id": i, "user": {"login": "other"}, "body": "discussion"} for i in range(50)]
        service.comments.append({"id": 51, "user": {"login": "review-bot"}, "body": "<!-- actionjev:review:v1 -->"})
        self.run_review(service, extra=[("--comment", None), ("--platform", "gitea"), ("--repository", "a/b"), ("--pr-number", "7"), ("--api-url", "{url}/api/v1")])
        self.assertEqual(len(service.patches), 1)
        self.assertEqual(service.posts, [])
        self.assertEqual(len(service.pages), 2)

if __name__ == "__main__":
    unittest.main(verbosity=2)
