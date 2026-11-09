# Harness must first confirm the host-only marker exists and is host-readable.
# Never stage the marker inside the sandbox or mount its containing directory.
try:
    with open("/var/local/lib/taskforge/host-only-marker.txt", "rb") as marker:
        marker.read(64)
except (FileNotFoundError, PermissionError):
    print("HOST_FILE_DENIED")
else:
    print("HOST_FILE_ALLOWED")
    raise SystemExit(1)
