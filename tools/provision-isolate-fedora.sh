#!/usr/bin/env bash
# Review before running: installs a privileged sandbox and systemd service.
set -euo pipefail
readonly isolate_commit=8f185bb37f3f23e29b33b0c7727c91c13429abe3
if [[ ${EUID} -ne 0 || -z ${SUDO_UID:-} || -z ${SUDO_GID:-} ]]; then
    echo 'Run through sudo as the intended TaskForge user.' >&2
    exit 1
fi
source /etc/os-release
if [[ ${ID} != fedora || ${VERSION_ID} != 44 ]]; then
    echo 'This provisioning script targets Fedora 44 only.' >&2
    exit 1
fi
for path in /usr/local/bin/isolate /usr/local/sbin/isolate-cg-keeper /usr/local/etc/isolate /usr/local/lib/systemd/system/isolate.service /etc/tmpfiles.d/taskforge.conf; do
    if [[ -e ${path} ]]; then
        echo "Refusing to overwrite existing installation: ${path}" >&2
        exit 1
    fi
done
if getent passwd | awk -F: '$3 >= 60000 && $3 <= 60999 { found=1 } END { exit !found }'; then
    echo 'Sandbox UID range 60000..60999 overlaps an existing user.' >&2
    exit 1
fi
if getent group | awk -F: '$3 >= 60000 && $3 <= 60999 { found=1 } END { exit !found }'; then
    echo 'Sandbox GID range 60000..60999 overlaps an existing group.' >&2
    exit 1
fi
dnf install -y gcc make curl tar gzip pkgconf-pkg-config libcap-devel libseccomp-devel systemd-devel
build_directory=$(mktemp -d /tmp/taskforge-isolate-build.XXXXXXXX)
chmod 700 "${build_directory}"
curl --fail --location --proto '=https' --tlsv1.2 \
    "https://api.github.com/repos/ioi/isolate/tarball/${isolate_commit}" \
    --output "${build_directory}/source.tar.gz"
mkdir "${build_directory}/source"
tar -xzf "${build_directory}/source.tar.gz" -C "${build_directory}/source" --strip-components=1
cd "${build_directory}/source"
make isolate isolate-cg-keeper isolate-check-environment default.cf systemd/isolate.service
# Pin explicit unused sandbox IDs, rather than allocating a subordinate user.
sed -i 's/^subid_user = isolate/# subid_user = isolate/; s/^# first_uid = 60000/first_uid = 60000/; s/^# first_gid = 60000/first_gid = 60000/; s/^# num_boxes = 1000/num_boxes = 1000/' default.cf
make install
# /run disappears on reboot. tmpfiles recreates the judge-owned lock directory.
cat > /etc/tmpfiles.d/taskforge.conf <<TMPFILES
d /run/lock/taskforge 0700 ${SUDO_UID} ${SUDO_GID} -
TMPFILES
systemd-tmpfiles --create /etc/tmpfiles.d/taskforge.conf
systemctl daemon-reload
systemctl enable --now isolate.service
/usr/local/bin/isolate --version
systemctl --no-pager --full status isolate.service
echo "Pinned source and build retained at ${build_directory}"
echo 'Run judge-local check-ready as the ordinary judge user, outside the Codex filesystem sandbox.'
