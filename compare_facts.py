#!/usr/bin/env python3
"""Compare two facter JSON output files, ignoring key order."""

import json
import sys


def compare(a, b, path=""):
    if type(a) != type(b):
        print(f"  type mismatch at {path!r}: {type(a).__name__} vs {type(b).__name__}")
        return
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            child = f"{path}.{k}" if path else k
            if k not in b:
                print(f"  only in old: {child}")
            elif k not in a:
                print(f"  only in new: {child}")
            else:
                compare(a[k], b[k], child)
    elif isinstance(a, list):
        if len(a) != len(b):
            print(f"  length mismatch at {path!r}: {len(a)} vs {len(b)}")
        for i, (x, y) in enumerate(zip(a, b)):
            compare(x, y, f"{path}[{i}]")
    elif a != b:
        print(f"  differ: {path!r}")
        print(f"    old: {json.dumps(a)[:120]}")
        print(f"    new: {json.dumps(b)[:120]}")


def main():
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} old.json new.json")
        sys.exit(1)

    with open(sys.argv[1]) as f:
        old = json.load(f)
    with open(sys.argv[2]) as f:
        new = json.load(f)

    compare(old, new)


if __name__ == "__main__":
    main()
