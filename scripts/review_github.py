#!/usr/bin/env python3
"""Trusted GitHub-only command and status handling. Never receives a model key."""
import json
import os
from pathlib import Path
import re
import sys
import urllib.parse
import urllib.request

REPOSITORY = "AlexBabescu/ActionJev"
ROOT = f"https://api.github.com/repos/{REPOSITORY}"
MARKER = "<!-- actionjev:review:v1 -->"


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise ValueError("API redirects are not allowed")


def api(path, data=None, method=None):
    request = urllib.request.Request(ROOT + path, method=method,
        data=json.dumps(data).encode() if data is not None else None,
        headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                 "Accept": "application/vnd.github+json", "Content-Type": "application/json",
                 "User-Agent": "ActionJev", "X-GitHub-Api-Version": "2022-11-28"})
    opener = urllib.request.build_opener(NoRedirect(), urllib.request.ProxyHandler({}))
    with opener.open(request, timeout=30) as response:
        body = response.read(1_000_001)
    if len(body) > 1_000_000:
        raise ValueError("API response too large")
    return json.loads(body) if body else None


def number(value):
    if not re.fullmatch(r"[1-9][0-9]{0,9}", str(value)):
        raise ValueError("Invalid PR number")
    return int(value)


def validate_pr(pr):
    data = api(f"/pulls/{pr}")
    if (data.get("number") != pr or data.get("state") != "open"
            or data.get("base", {}).get("repo", {}).get("full_name") != REPOSITORY):
        raise ValueError("Expected an open PR targeting ActionJev")
    return data


def dispatch(event):
    # Author associations alone are not authorization. Check current permissions.
    comment = event.get("comment", {})
    issue = event.get("issue", {})
    sender = event.get("sender", {})
    if (event.get("action") != "created" or comment.get("body", "").strip() != "/jev review"
            or not issue.get("pull_request")
            or event.get("repository", {}).get("full_name") != REPOSITORY
            or sender.get("id") is None or sender.get("id") != comment.get("user", {}).get("id")
            or sender.get("type") != "User"):
        return False
    login = sender.get("login", "")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]{0,38}", login):
        return False
    permissions = api(f"/collaborators/{urllib.parse.quote(login, safe='')}/permission")
    if permissions.get("permission") != "admin" and permissions.get("role_name") not in ("admin", "maintain"):
        return False
    pr = number(issue.get("number"))
    validate_pr(pr)
    api("/actions/workflows/jev-review.yml/dispatches",
        {"ref": "main", "inputs": {"pr-number": str(pr)}})
    return True


def run_url():
    run = os.environ["GITHUB_RUN_ID"]
    if not re.fullmatch(r"[1-9][0-9]*", run):
        raise ValueError("Invalid run ID")
    return f"https://github.com/{REPOSITORY}/actions/runs/{run}"


def status(pr, failed=False, approval_required=True):
    url = run_url()
    pending = f"<!-- actionjev:pending:{os.environ['GITHUB_RUN_ID']} -->"
    comment = None
    for page in range(1, 21):
        entries = api(f"/issues/{pr}/comments?per_page=100&page={page}")
        for entry in entries:
            if (entry.get("user", {}).get("login") == "github-actions[bot]"
                    and entry.get("body", "").startswith(MARKER)):
                comment = entry
                break
        if comment or len(entries) < 100:
            break
    else:
        raise ValueError("Comment pagination limit reached")
    if failed:
        # An older cancelled run must not overwrite a newer result or request.
        if not comment or pending not in comment["body"]:
            return
        message = "Review did not publish a result. The PR may have changed, the job may have failed or been cancelled, or approval may have been rejected. Check the workflow run before retrying."
    elif approval_required:
        message = "Review requested. Open the workflow run and approve the jev-approval environment to start. This comment will update when the review finishes."
    else:
        message = "Review starting automatically for the repository owner's PR. This comment will update when the review finishes."
    body = f"{MARKER}\n## ActionJev review\n\n{message}\n\n[View workflow run]({url})\n"
    if not failed:
        body += pending + "\n"
    if comment:
        api(f"/issues/comments/{number(comment['id'])}", {"body": body}, "PATCH")
    else:
        api(f"/issues/{pr}/comments", {"body": body})


def main():
    if (os.environ.get("GITHUB_REPOSITORY") != REPOSITORY
            or os.environ.get("GITHUB_REF") != "refs/heads/main"
            or os.environ.get("TYPESAFE_API_KEY") or os.environ.get("ACTIONJEV_TOKEN")):
        raise ValueError("Expected trusted main workflow without model credentials")
    command = sys.argv[1]
    if command == "dispatch" and os.environ.get("GITHUB_EVENT_NAME") == "issue_comment":
        event_path = Path(os.environ["GITHUB_EVENT_PATH"])
        if event_path.stat().st_size > 1_000_000:
            raise ValueError("Event too large")
        print("Review requested." if dispatch(json.loads(event_path.read_text())) else "Comment is not an authorized review command.")
    elif command in ("request", "failure") and os.environ.get("GITHUB_EVENT_NAME") == "workflow_dispatch":
        pr = number(os.environ.get("PR_NUMBER", ""))
        required = True
        if command == "request":
            data = validate_pr(pr)
            # The PR author's immutable account ID controls approval, never the
            # event sender, dispatch actor, branch name, or author association.
            required = data.get("user", {}).get("id") != 695992
            with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
                output.write(f"approval-required={str(required).lower()}\n")
        status(pr, failed=command == "failure", approval_required=required)
    else:
        raise ValueError("Unexpected command or event")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"Review request/status failed ({type(exc).__name__}). Check the PR and workflow permissions.", file=sys.stderr)
        sys.exit(1)
