#!/bin/sh
# Install the Super Champ X2 hidraw rule for the logged-in desktop user.
#
# Run from anywhere:
#   sudo packaging/udev/install.sh
#
# The rule must be numbered before 73-seat-late.rules, which is the rule that
# applies the `uaccess` ACL. Older revisions installed it as 99-scx2.rules;
# that tag arrives after the builtin has already run on a normal plug, so the
# ACL only appeared after a manual `udevadm trigger --action=add`. Remove that
# old file when upgrading.

set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "run as root: sudo $0" >&2
    exit 1
fi

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
rule=/etc/udev/rules.d/70-scx2.rules

# sudo sets SUDO_USER; pkexec sets PKEXEC_UID. Fall back to the current user
# when the script is run from a root shell.
user=${SUDO_USER:-}
if [ -z "$user" ] && [ -n "${PKEXEC_UID:-}" ]; then
    user=$(getent passwd "$PKEXEC_UID" | cut -d: -f1)
fi
[ -n "$user" ] || user=$(id -un)

install -m 0644 "$here/70-scx2.rules" "$rule"
rm -f /etc/udev/rules.d/99-scx2.rules

udevadm control --reload-rules
udevadm trigger --action=add --subsystem-match=hidraw
udevadm settle

echo "installed $rule"
echo
if [ -e /dev/hidraw0 ]; then
    echo "check that the amp node has user:$user:rw-:"
    for node in /dev/hidraw*; do
        [ -e "$node" ] || continue
        echo "  $node"
        getfacl "$node" 2>/dev/null | sed 's/^/    /' || ls -l "$node"
    done
else
    echo "no /dev/hidraw* yet; plug the amp in, then run: scx2 scan"
fi
