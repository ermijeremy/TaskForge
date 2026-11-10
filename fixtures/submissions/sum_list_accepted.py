import sys

n, *values = map(int, sys.stdin.buffer.read().split())
assert len(values) == n
print(sum(values))
