#!/usr/bin/env python3
"""Clean up isolated screenshot/test environments under /tmp/opencode/ghs-*.

Each KDE/GPUI screenshot environment starts an xdg-desktop-portal and
ksecretd bound to its own XDG directories. Those daemons can outlive the app and
recreate their data/kwalletd directory after the environment is deleted. This
script kills only processes whose environment points into that temp prefix and
optionally removes the disposable environment directories. It never touches the
real desktop's portal, secret service, or session bus.
"""

from __future__ import annotations

import argparse
import glob
import os
import shutil
import signal
import time
from collections import Counter

PREFIX = b"/tmp/opencode/ghs-"
EPHEMERAL_PATTERNS = (
    "ghs-m5-*",
    "ghs-m6-*",
    "ghs-settings-*",
    "ghs-viewer-env-*",
    "ghs-shot-env*",
    "ghs-shot-settings-*",
)


def find_victims() -> list[tuple[int, str]]:
    victims: list[tuple[int, str]] = []
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            env = open(f"/proc/{pid}/environ", "rb").read()
        except OSError:
            continue
        if PREFIX not in env:
            continue
        try:
            comm = open(f"/proc/{pid}/comm").read().strip()
        except OSError:
            comm = "?"
        victims.append((int(pid), comm))
    return victims


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--apply",
        action="store_true",
        help="kill the processes and remove the disposable environment dirs",
    )
    args = parser.parse_args()

    victims = find_victims()
    print(f"found {len(victims)} test process(es) with XDG paths under {PREFIX.decode()}")
    if victims:
        for comm, count in Counter(comm for _, comm in victims).most_common():
            print(f"  {count:4}  {comm}")

    if not args.apply:
        print("dry run; pass --apply to kill them and remove the temp env dirs")
        return 0

    for pid, _ in victims:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    time.sleep(0.5)
    for pid, _ in victims:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    removed = 0
    for pattern in EPHEMERAL_PATTERNS:
        for path in glob.glob("/tmp/opencode/" + pattern):
            if os.path.isdir(path):
                shutil.rmtree(path, ignore_errors=True)
                removed += 1
                print(f"removed {path}")

    time.sleep(0.5)
    remaining = find_victims()
    print(f"remaining test process(es): {len(remaining)}; removed {removed} dir(s)")
    return 1 if remaining else 0


if __name__ == "__main__":
    raise SystemExit(main())
