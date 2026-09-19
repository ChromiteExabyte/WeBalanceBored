#!/usr/bin/env bash
set -euo pipefail
app_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if [[ ! -f "$app_dir/70-we-balance-bored.rules" ]]; then
    echo 'Extract the full download before running setup.' >&2
    exit 1
fi
for command in udevadm modprobe install; do
    if ! command -v "$command" >/dev/null; then
        echo "Missing $command. This setup requires a Linux desktop using udev." >&2
        exit 1
    fi
done
if [[ $EUID -ne 0 ]]; then
    echo 'One-time setup: install device-access rules and enable uinput at boot.'
    echo 'The app itself will run as your normal desktop user.'
    exec sudo bash "$0"
fi
modprobe uinput
modprobe hid_wiimote
install -m 0644 "$app_dir/70-we-balance-bored.rules" /etc/udev/rules.d/70-we-balance-bored.rules
install -d -m 0755 /etc/modules-load.d
printf 'uinput\nhid_wiimote\n' > /etc/modules-load.d/we-balance-bored.conf
udevadm control --reload-rules
udevadm trigger --subsystem-match=misc --sysname-match=uinput
udevadm trigger --subsystem-match=input
udevadm settle
echo 'Done. Reconnect the board, then launch WeBalanceBored as your normal user.'
echo 'If access is still denied, log out of the desktop and log back in.'
