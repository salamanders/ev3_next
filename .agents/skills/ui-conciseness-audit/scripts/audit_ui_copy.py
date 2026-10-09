#!/usr/bin/env python3
"""Audit web templates and scripts for redundant copy and forbidden filler words."""

import re
import sys
from collections import Counter
from pathlib import Path

FORBIDDEN_WORDS = ["mode"]
FILLER_CANDIDATES = ["widget", "state", "item", "value", "target", "button"]


def audit_html_file(file_path: Path):
    content = file_path.read_text(encoding="utf-8")
    # Strip HTML tags to inspect visible text
    text_only = re.sub(r"<[^>]+>", " ", content)
    words = re.findall(r"\b[A-Za-z]{3,}\b", text_only.lower())

    print(f"\n--- Auditing: {file_path} ---")

    # 1. Check for forbidden words in visible text
    for forbidden in FORBIDDEN_WORDS:
        matches = [w for w in words if w == forbidden]
        if matches:
            print(f"  [FAIL] Found {len(matches)} instance(s) of forbidden word: '{forbidden}'")
        else:
            print(f"  [PASS] Zero instances of forbidden word: '{forbidden}'")

    # 2. Check top word frequencies
    counts = Counter(words)
    print("  Top recurring words in visible text:")
    for word, count in counts.most_common(10):
        if count > 2:
            print(f"    - {word}: {count} occurrences")


def main():
    target_dir = Path("web_assets")
    if not target_dir.exists():
        target_dir = Path(".")

    html_files = list(target_dir.glob("*.html"))
    if not html_files:
        print("No HTML files found to audit.")
        sys.exit(0)

    for hf in sorted(html_files):
        audit_html_file(hf)


if __name__ == "__main__":
    main()
