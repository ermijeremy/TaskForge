# 768 KiB per stream, 1536 KiB combined. Each stream alone stays below 1 MiB.
import sys

chunk = b"x" * (32 * 1024)
for _ in range(24):
    sys.stdout.buffer.write(chunk)
    sys.stdout.buffer.flush()
    sys.stderr.buffer.write(chunk)
    sys.stderr.buffer.flush()
