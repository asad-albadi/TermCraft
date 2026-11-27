#!/bin/sh
set -eu
device=$(losetup -f)
number=${device#/dev/loop}
test -b "$device" || mknod "$device" b 7 "$number"
truncate -s 64M /tmp/rdp-fixture.img
mkfs.ext4 -q /tmp/rdp-fixture.img
losetup "$device" /tmp/rdp-fixture.img
trap 'losetup -d "$device"' EXIT
mkdir /fixture
mount "$device" /fixture
trap 'umount /fixture; losetup -d "$device"' EXIT
TMPDIR=/fixture /test-binary "$@" --nocapture
