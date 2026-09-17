#!/usr/bin/env python3
"""Fetch an ActionJev PR as Git objects only; never check out or run PR code.

This repository-specific helper runs from the trusted workflow commit on main.
It needs a GitHub token for metadata, but must not receive the TypeSafe key.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import urllib.request

REPOSITORY = "AlexBabescu/ActionJev"
REMOTE = f"https://github.com/{REPOSITORY}.git"
API = f"https://api.github.com/repos/{REPOSITORY}/pulls/"
MAX_RESPONSE_BYTES = 1_000_000


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("GitHub API redirects are not allowed")


def pr_number(value):
    if not isinstance(value, str) or not re.fullmatch(r"[1-9][0-9]{0,9}", value):
        raise ValueError("PR number must be a positive ASCII integer of at most 10 digits")
    return int(value)


def validate_metadata(data, number):
    if not isinstance(data, dict) or data.get("number") != number or data.get("state") != "open":
        raise ValueError("Expected the requested open pull request")
    base, head = data.get("base", {}), data.get("head", {})
    if base.get("repo", {}).get("full_name") != REPOSITORY:
        raise ValueError("Only pull requests targeting ActionJev can be reviewed")
    commits = (base.get("sha"), head.get("sha"))
    if not all(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{40}", sha) for sha in commits):
        raise ValueError("GitHub returned an invalid commit ID")
    # Deliberately ignore PR branch names, URLs, body, title and head repository.
    return commits


def metadata(number):
    token = os.environ.get("GH_TOKEN", "")
    if not token:
        raise ValueError("A step-scoped GitHub token is required")
    request = urllib.request.Request(API + str(number), headers={
        "Authorization": "Bearer " + token,
        "Accept": "application/vnd.github+json",
        "User-Agent": "ActionJev-trusted-review",
        "X-GitHub-Api-Version": "2022-11-28",
    })
    opener = urllib.request.build_opener(NoRedirect(), urllib.request.ProxyHandler({}))
    with opener.open(request, timeout=30) as response:
        raw = response.read(MAX_RESPONSE_BYTES + 1)
    if len(raw) > MAX_RESPONSE_BYTES:
        raise ValueError("GitHub PR metadata exceeded the response limit")
    return json.loads(raw)


def git(repo, *args):
    # No inherited tokens, credential helpers, Git overrides, proxy or user config.
    env = {
        "PATH": os.defpath,
        "HOME": str(repo),
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_LFS_SKIP_SMUDGE": "1",
    }
    command = ["git", "-C", str(repo), "-c", "core.hooksPath=/dev/null",
               "-c", "core.fsmonitor=false", "-c", "credential.helper=",
               "-c", "protocol.allow=never", "-c", "protocol.https.allow=always",
               *args]
    result = subprocess.run(command, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, text=True, timeout=180)
    if result.returncode:
        raise ValueError("Git object fetch/verification failed; no review was started")
    return result.stdout.strip()


def prepare(data, number, temp_root):
    base, head = validate_metadata(data, number)
    repo = Path(tempfile.mkdtemp(prefix="actionjev-pr-", dir=temp_root))
    git(repo, "init", "--bare", "--template=")
    # Public fixed origin, not a fork-supplied URL. Full history for merge-base.
    git(repo, "fetch", "--no-tags", "--no-recurse-submodules", "--no-auto-maintenance",
        REMOTE, f"+{base}:refs/actionjev/base",
        f"+refs/pull/{number}/head:refs/actionjev/head")
    if git(repo, "rev-parse", "refs/actionjev/base^{commit}") != base:
        raise ValueError("Base commit verification failed")
    if git(repo, "rev-parse", "refs/actionjev/head^{commit}") != head:
        raise ValueError("PR head changed during preparation; rerun the workflow")
    git(repo, "merge-base", base, head)
    return {"path": str(repo), "base": base, "head": head, "pr-number": str(number)}


def main():
    expected = {"GITHUB_REPOSITORY": REPOSITORY,
                "GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_REF": "refs/heads/main"}
    if any(os.environ.get(key) != value for key, value in expected.items()):
        raise ValueError("This helper is restricted to manual ActionJev/main workflows")
    if os.environ.get("TYPESAFE_API_KEY") or os.environ.get("ACTIONJEV_TOKEN"):
        raise ValueError("Do not pass reviewer credentials to the preparation step")
    number = pr_number(os.environ.get("PR_NUMBER", ""))
    result = prepare(metadata(number), number, os.environ["RUNNER_TEMP"])
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for key, value in result.items():
            if "\n" in value or "\r" in value:
                raise ValueError("Invalid workflow output")
            output.write(f"{key}={value}\n")
    print(f"Prepared PR #{number} as Git objects; no PR files checked out or executed.")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        # Never log API responses, attacker-controlled text, URLs or credentials.
        print(f"PR preparation failed ({type(exc).__name__}); check PR number, main branch and permissions.", file=sys.stderr)
        sys.exit(1)
