#!/usr/bin/env python3
"""Verify an off-tag compiler identifies the committed source being validated."""
import pathlib
import re
import subprocess
import sys


def matches(line, head):
    match = re.fullmatch(r"wolf 0\.2\.37\+dev\.([0-9a-f]{7,40}) \(wolfgang, pin ([0-9a-f]{7,40})\)", line)
    return bool(re.fullmatch(r"[0-9a-f]{40}", head) and match
                and match[1] == match[2] and head.startswith(match[1]))


def main():
    binary, head = sys.argv[1:]
    if not pathlib.Path(binary).is_absolute():
        raise SystemExit("Build stamp check requires an absolute compiler path")
    result = subprocess.run([binary, "--version"], capture_output=True, check=True)
    text = result.stdout.decode()
    print(text, end="")
    if not text.splitlines() or not matches(text.splitlines()[0], head):
        raise SystemExit("Compiler build stamp does not match the committed validation head")


if __name__ == "__main__":
    main()
