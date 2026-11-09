# Trusted, fixed readiness probe. Never accepts submitted source or input.
import errno
import os
import platform
import socket

assert platform.python_version() == "3.14.8"
assert os.getuid() != 0
for path in ("/etc/passwd", "/proc/self/environ", "/run/docker.sock"):
    try:
        open(path, "rb").close()
    except OSError:
        pass
    else:
        raise AssertionError(f"unexpected host access: {path}")
for path in ("/box/probe", "/usr/probe", "/tmp/probe"):
    try:
        open(path, "wb").close()
    except OSError:
        pass
    else:
        raise AssertionError(f"unexpected write access: {path}")
try:
    pid = os.fork()
except OSError as error:
    assert error.errno in (errno.EAGAIN, errno.EPERM)
else:
    if pid == 0:
        os._exit(0)
    os.waitpid(pid, 0)
    raise AssertionError("subprocess creation was allowed")
with socket.socket() as connection:
    connection.settimeout(0.2)
    try:
        connection.connect(("192.0.2.1", 9))
    except OSError as error:
        # A timeout/refused connection does not prove network isolation.
        assert error.errno in (errno.ENETUNREACH, errno.EACCES, errno.EPERM)
    else:
        raise AssertionError("external network was reachable")
print("TASKFORGE_SMOKE_OK " + platform.python_version())
