"""Read-only native audit for the pinned Isolate installation; never cleans slots."""
import argparse
import fcntl
import json
import os
import stat
from contextlib import ExitStack
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    args = parser.parse_args()
    config_path = args.config.resolve(strict=True)
    settings = json.loads(config_path.read_text())
    sandbox = settings.get("sandbox", {})
    first = sandbox.get("box_id_start", 0)
    last = sandbox.get("box_id_end", 9)
    if type(first) is not int or type(last) is not int or not 0 <= first <= last <= 999:
        raise ValueError("invalid sandbox ID range")
    locks = config_path.parent / sandbox.get("lock_directory", "/run/lock/taskforge")
    mode = locks.stat().st_mode
    if not stat.S_ISDIR(mode) or mode & 0o022:
        raise ValueError("lock directory is not private")
    cgroot = Path(Path("/run/isolate/cgroup").read_text().strip())
    if not cgroot.is_absolute() or not cgroot.is_dir():
        raise ValueError("missing delegated cgroup root")
    with ExitStack() as opened:
        for box_id in range(first, last + 1):
            fd = os.open(locks / f"box-{box_id}.lock", os.O_RDWR | os.O_CREAT | os.O_CLOEXEC | os.O_NOFOLLOW, 0o600)
            lock = opened.enter_context(os.fdopen(fd, "r+"))
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                raise ValueError("slot lock is not a regular file")
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            for path in [Path("/var/local/lib/isolate") / str(box_id),
                         cgroot / f"box-{box_id}", locks / f"box-{box_id}.quarantine"]:
                if path.exists():
                    raise RuntimeError(f"slot {box_id} still has sandbox resources or quarantine")
        # The documented installation reserves UID 60000 + box ID for submissions.
        # Confirm that premise before scanning for surviving submission identities.
        config_lines = Path("/usr/local/etc/isolate").read_text().splitlines()
        first_uid = [line.split("=", 1)[1].strip() for line in config_lines
                     if line.split("=", 1)[0].strip() == "first_uid" and "=" in line]
        if first_uid != ["60000"]:
            raise ValueError("process audit requires the documented first_uid = 60000")
        reserved = set(range(60000 + first, 60000 + last + 1))
        for process in Path("/proc").iterdir():
            if process.name.isdecimal():
                try:
                    if process.stat().st_uid in reserved:
                        raise RuntimeError("a reserved sandbox identity still has a process")
                except (FileNotFoundError, ProcessLookupError):
                    pass
    print("Clean-slot audit passed: locks available; no boxes, cgroups, quarantine markers or reserved-UID processes.")


if __name__ == "__main__":
    main()
