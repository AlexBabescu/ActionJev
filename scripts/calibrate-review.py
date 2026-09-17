#!/usr/bin/env python3
"""Live model check, opt-in behind jev-api approval. Never executes fixture code."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    binary, policy = (str(Path(arg).resolve()) for arg in sys.argv[1:])
    failed = False
    rows = ["## Rounding review calibration", "",
            "This small check compares broken and corrected examples. It is not a measure of general review accuracy.", "",
            "| Example | Correctness screening score | Actionable correctness finding | Expected |",
            "|---|---:|---|---|"]
    with tempfile.TemporaryDirectory(prefix="jev-calibration-") as temp:
        for labelled in (False, True):
            for broken in (True, False):
                name = ("labelled" if labelled else "neutral") + ("-broken" if broken else "-corrected")
                root = Path(temp) / name
                root.mkdir()
                def git(*args):
                    return subprocess.check_output(["git", "-C", str(root), *args], text=True, stderr=subprocess.DEVNULL).strip()
                git("init", "-b", "main")
                git("config", "user.name", "Review calibration")
                git("config", "user.email", "calibration@example.invalid")
                git("commit", "--allow-empty", "-qm", "base")
                base = git("rev-parse", "HEAD")
                prefix = '"""Deliberately flawed, non-production input for a review smoke test."""\n\n' if labelled else ""
                expression = "total_items // page_size" if broken else "(total_items + page_size - 1) // page_size"
                (root / "pages.py").write_text(prefix + 'def page_count(total_items: int, page_size: int) -> int:\n'
                    '    """Return pages needed, counting a nonempty partial page as a full page."""\n'
                    '    if total_items < 0:\n        raise ValueError("total_items must be non-negative")\n'
                    '    if page_size <= 0:\n        raise ValueError("page_size must be positive")\n'
                    f'    return {expression}\n')
                git("add", "pages.py")
                git("commit", "-qm", "example")
                output = root / "report"
                result = subprocess.run([binary, "--repo", str(root), "--base", base, "--head", "HEAD",
                    "--platform", "none", "--policy", policy, "--max-files", "1", "--max-followups", "5",
                    "--concurrency", "1", "--output-dir", str(output)],
                    capture_output=True, timeout=240)
                if result.returncode:
                    # Do not expose subprocess output, source text or API bodies.
                    raise RuntimeError(f"Calibration failed for {name}, exit {result.returncode}")
                report = json.loads((output / "report.json").read_text())
                score = next(s["probability"] for s in report["signals"] if s["dimension"] == "correctness")
                found = any(f["actionable"] and f["dimension"] == "correctness" for f in report["findings"])
                failed |= found != broken
                rows.append(f"| {name} | {score:.3f} | {found} | {broken} |")
    summary = "\n".join(rows) + "\n"
    print(summary)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as file:
            file.write(summary)
    if failed:
        raise RuntimeError("Model calibration failed. Inspect the scores before changing the policy or thresholds.")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"Calibration did not pass ({type(exc).__name__}).", file=sys.stderr)
        sys.exit(1)
