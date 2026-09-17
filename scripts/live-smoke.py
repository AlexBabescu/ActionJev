#!/usr/bin/env python3
"""A tiny synthetic fixture. Never execute the reviewed code or print credentials."""
import json
import os
from pathlib import Path
import subprocess
import sys

if sys.argv[1] == "setup":
    root = Path(sys.argv[2]).resolve()
    root.mkdir(exist_ok=False)
    def git(*args):
        return subprocess.check_output(["git", "-C", str(root), *args], text=True, stderr=subprocess.DEVNULL).strip()
    git("init", "-b", "main")
    git("config", "user.name", "ActionJev smoke fixture")
    git("config", "user.email", "smoke@example.invalid")
    source = root / "auth.py"
    source.write_text('def allowed(is_admin: bool) -> bool:\n    """Only administrators are allowed."""\n    return is_admin\n')
    git("add", "auth.py")
    git("commit", "-qm", "Correct authorization")
    base = git("rev-parse", "HEAD")
    source.write_text('def allowed(is_admin: bool) -> bool:\n    """Only administrators are allowed."""\n    return True\n')
    git("add", "auth.py")
    git("commit", "-qm", "Synthetic authorization regression")
    policy = json.loads((Path(__file__).resolve().parents[1] / "prompts/review.json").read_text())
    policy["dimensions"] = [{"id": "security", "statement": "Does the changed allowed function grant access to non-administrators contrary to its explicit docstring?"}]
    policy_path = root.parent / "smoke-policy.json"
    policy_path.write_text(json.dumps(policy))
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        for key, value in {"path": root, "base": base, "head": git("rev-parse", "HEAD"), "policy": policy_path}.items():
            print(f"{key}={value}", file=output)
elif sys.argv[1] == "verify":
    report = json.loads(Path(sys.argv[2]).read_text())
    traces = [json.loads(line) for line in Path(sys.argv[3]).read_text().splitlines()]
    assert not report["dry_run"] and report["complete"] and report["scanned_files"] == 1
    kinds = {q["type"] for trace in traces for q in trace["questions"].values()}
    assert kinds == {"noul", "choice", "score"}, "Live fixture did not exercise all three typed API primitives"
    assert report["api_calls"] == 3, "Expected one bounded screen, locate and judge request"
    print(f"Live Rust/composite integration passed: Noul, Choice, Score; {report['api_calls']} API calls; {report['input_tokens']} input tokens.")
else:
    raise SystemExit("Expected setup or verify")
