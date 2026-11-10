# Attempts exactly one benign child; cleans it up if the policy permits it.
import errno
import os

try:
    child = os.fork()
except OSError as error:
    if error.errno not in (errno.EAGAIN, errno.EPERM):
        raise
    print("SUBPROCESS_DENIED")
else:
    if child == 0:
        os._exit(0)
    os.waitpid(child, 0)
    print("SUBPROCESS_ALLOWED")
    raise SystemExit(1)
