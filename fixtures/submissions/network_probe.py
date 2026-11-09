# Numeric documentation address: no DNS or dependency on a live remote service.
import errno
import socket

try:
    with socket.socket() as connection:
        connection.settimeout(0.25)
        connection.connect(("192.0.2.1", 9))
except OSError as error:
    if error.errno not in (errno.ENETUNREACH, errno.EACCES, errno.EPERM):
        raise RuntimeError("network denial was not established") from error
    print("NETWORK_DENIED")
else:
    print("NETWORK_ALLOWED")
    raise SystemExit(1)
