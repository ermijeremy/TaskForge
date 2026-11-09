# Finite 4 MiB output; exceeds the default 1 MiB combined output budget.
import sys

chunk = b"x" * (64 * 1024)
for _ in range(64):
    sys.stdout.buffer.write(chunk)
    sys.stdout.buffer.flush()
