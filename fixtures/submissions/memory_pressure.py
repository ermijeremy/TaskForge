# At most 256 MiB of retained, touched data; use the 128 MiB sandbox limit.
# MemoryError alone is not evidence of an OOM kill; inspect trusted metadata.
chunk = b"x" * (1024 * 1024)
blocks = []
for _ in range(256):
    blocks.append(bytearray(chunk))
print("allocation completed")
