# USB Enablement Procedure for Bitmain Amlogic Controlboards

This directory collects the validated procedure for enabling Linux USB
host and USB mass storage on the Bitmain Amlogic controlboard used in this
repository, plus the current work-in-progress path for USB Wi-Fi.

The currently validated working combination for USB host and storage is:

- kernel image: `usb/bin/Image-usb-storage`
- DTB: `usb/bin/axg_s400_antminer.usb-host-nand-clocks.dtb`

With that pair installed on a compatible board, the board:

- boots normally from UBIFS root on `ubi0:nvdada_log`
- initializes `mtd6` correctly
- binds `usb-storage`
- exposes `/dev/sda` and `/dev/sda1`
- passes live USB read/write testing

For USB Wi-Fi, the currently identified dongle is:

- USB ID: `0bda:c811`
- vendor: `Realtek`
- product string: `802.11ac NIC`
- driver family: `RTL8821CU` / `RTL8811CU`

The public Amlogic 4.9 tree in this repository does not contain an in-tree
driver for that device. The supported path here is an external `rtl8821cu`
module build.

On the validated storage-only kernel (`usb/bin/Image-usb-storage`), the external
module currently fails to load on this board with `Unknown symbol
wireless_send_event`. The supported path for `0bda:c811` is therefore the
Wi-Fi-capable kernel plus a cfg80211-enabled `8821cu.ko` build.

## What is in this directory

- `build-usb-storage-kernel.sh`
  Builds the validated USB-storage kernel image.
- `build-usb-wifi-kernel.sh`
  Builds a Wi-Fi-capable kernel image with `cfg80211` and WEXT enabled.
- `build-rtl8821cu-module.sh`
  Builds the external `rtl8821cu` module for the `0bda:c811` Realtek dongle.
- `build-wifi-userspace.sh`
  Builds armhf `wpa_supplicant`, `wpa_cli`, `wpa_passphrase`, and the required
  `libnl` runtime libraries for the board.
- `install-kernel-image.sh`
  Installs a kernel image onto the board.
- `install-usb-dtb.sh`
  Installs the validated DTB onto the board.
- `install-rtl8821cu.sh`
  Uploads and loads the external `rtl8821cu` kernel module.
- `install-wifi-userspace.sh`
  Uploads `wpa_supplicant`, `wpa_cli`, `wpa_passphrase`, and the matching
  `libnl` runtime libraries to the board.
- `connect-wifi.sh`
  Starts `wpa_supplicant` on the board and requests DHCP on `wlan0` without
  touching the existing `eth0` DHCP client.

## Prerequisites

Host machine:

- macOS or Linux shell environment
- `docker`
- `sshpass`
- `shasum`
- `dtc` if you want to rebuild DTBs by hand

Board assumptions:

- board reachable over SSH as `root@<board-ip>`
- password is known, usually `root`
- serial console available for recovery is strongly recommended

Install host prerequisites on macOS if needed:

```sh
brew install dtc rg sshpass
open -a Docker
```

## Files and artifacts used by this workflow

Everything needed for the workflow is in `usb/`, with generated artifacts under
`usb/bin/`:

- extracted vendor baseline config:
  `usb/Antminer-4.9.241.config`
- validated rebuilt kernel:
  `usb/bin/Image-usb-storage`
- kernel checksum:
  `usb/bin/Image-usb-storage.sha256`
- validated DTB for rebuilt kernel:
  `usb/bin/axg_s400_antminer.usb-host-nand-clocks.dtb`
- DTB checksum:
  `usb/bin/axg_s400_antminer.usb-host-nand-clocks.dtb.sha256`

Generated on demand:

- Wi-Fi-capable kernel image:
  `usb/bin/Image-usb-storage-wifi`
- external Realtek module:
  `usb/bin/8821cu.ko`
- armhf Wi-Fi userspace tools:
  `usb/bin/wpa_supplicant-armhf`
  `usb/bin/wpa_cli-armhf`
  `usb/bin/wpa_passphrase-armhf`
  `usb/bin/libnl-3.so.200`
  `usb/bin/libnl-genl-3.so.200`

The validated storage-only kernel remains the right baseline for USB mass
storage, but it is not sufficient by itself for this external Realtek Wi-Fi
module on the live board.

Use the NAND-clock DTB with the rebuilt kernel.

## Step 1: Build the kernel

From the repository root:

```sh
usb/build-usb-storage-kernel.sh
```

What it does:

- starts a Docker build environment
- clones `LineageOS/android_kernel_amlogic_linux-4.9` into a persistent Docker volume if needed
- uses the extracted live board config as the base
- forces these options built in:
  - `CONFIG_SCSI=y`
  - `CONFIG_BLK_DEV_SD=y`
  - `CONFIG_USB_STORAGE=y`
  - `CONFIG_USB_UAS=y`

Build outputs:

- kernel image:
  `usb/bin/Image-usb-storage`
- kernel checksum:
  `usb/bin/Image-usb-storage.sha256`
- final config:
  `usb/bin/.config.final`
### Supported path on this board

The validated storage-only kernel does not export the wireless symbols this
driver needs, so build and install the Wi-Fi-capable kernel first:
- verifies the uploaded and installed checksums

By default, this only stages the image if `REBOOT_AFTER_INSTALL=0`.

## Step 3: Install the validated DTB

From the repository root:

```sh
BOARD_PASSWORD=root REBOOT_AFTER_INSTALL=1 \
  usb/install-usb-dtb.sh <board-ip>
```

Current default DTB:

```text
usb/bin/axg_s400_antminer.usb-host-nand-clocks.dtb
```

What it does:

- verifies the existing DTB on the board
- backs up `/axg_s400_antminer.dtb` to `/axg_s400_antminer.dtb.pre-usb-host`
- uploads the validated DTB
- verifies the installed checksum
- reboots using BusyBox-safe fallback:
  - `/sbin/reboot`
  - `/bin/busybox reboot`
  - `busybox reboot`

## Step 4: Verify the board after reboot

Check the running kernel and DTB checksum:

```sh
sshpass -p root ssh -o StrictHostKeyChecking=no root@<board-ip> '
uname -a
sha256sum /axg_s400_antminer.dtb
'
```

Expected signs of success:

- kernel reports the rebuilt 4.9.337 image
- DTB checksum matches `usb/bin/axg_s400_antminer.usb-host-nand-clocks.dtb`

Check NAND/UBI and USB storage in the log:

```sh
sshpass -p root ssh -o StrictHostKeyChecking=no root@<board-ip> '
dmesg | grep -Ei "aml_mtd_nand|UBI|ubi0|usb-storage|scsi host|sd[a-z]|failed to get gate|Cannot open root"
'
```

Expected successful lines include:

```text
ubi0: attached mtd6
UBIFS (ubi0:0): mounted UBI device 0, volume 0, name "nvdada_log"
VFS: Mounted root (ubifs filesystem) on device 0:17.
usb-storage 1-1:1.0: USB Mass Storage device detected
scsi host0: usb-storage 1-1:1.0
sd 0:0:0:0: [sda] Attached SCSI removable disk
```

## Step 5: Verify USB read/write

This minimal live test mounts `/dev/sda1`, writes a file, reads it back,
removes it, and unmounts cleanly:

```sh
sshpass -p root ssh -o StrictHostKeyChecking=no root@<board-ip> '
set -e
mkdir -p /mnt/usbtest
mount | grep -q "on /mnt/usbtest " || mount /dev/sda1 /mnt/usbtest
test_file=/mnt/usbtest/copilot-rw-test.txt
payload="usb-rw-ok $(date +%s)"
printf "%s\n" "$payload" > "$test_file"
sync
read_back=$(cat "$test_file")
[ "$read_back" = "$payload" ]
rm -f "$test_file"
sync
umount /mnt/usbtest
printf "rw_test=pass\ncleanup=pass\nunmount=pass\n"
'
```

## Recovery and rollback

### Roll back the kernel from Linux

If the board is still bootable:

```sh
sshpass -p root ssh -o StrictHostKeyChecking=no root@<board-ip> '
set -e
cp /Image.pre-usb-storage /Image
sync
/sbin/reboot || /bin/busybox reboot || busybox reboot
'
```

### Boot the backup kernel once from U-Boot

If the board does not boot Linux after a kernel install, use the serial console
and boot the backup kernel directly from UBIFS:

```text
setenv ker_addr 1080000
setenv dtb_addr 1000000

ubi part nvdata
ubifsmount ubi0:nvdada_log
ubifsload ${ker_addr} Image.pre-usb-storage
ubifsload ${dtb_addr} axg_s400_antminer.dtb

setenv bootargs "init=/sbin/init console=ttyS0,115200 no_console_suspend earlycon=aml_uart,0xff803000 jtag=disable root=ubi0:nvdada_log rootfstype=ubifs rw ubi.mtd=6,2048"

booti ${ker_addr} - ${dtb_addr}
```

After Linux comes up, restore `/Image` from `/Image.pre-usb-storage`.

### Roll back the DTB from Linux

```sh
sshpass -p root ssh -o StrictHostKeyChecking=no root@<board-ip> '
set -e
cp /axg_s400_antminer.dtb.pre-usb-host /axg_s400_antminer.dtb
sync
/sbin/reboot || /bin/busybox reboot || busybox reboot
'
```

## Recommended operator flow

For a fresh operator using this repository, the shortest validated path is:

1. `usb/build-usb-storage-kernel.sh`
2. `BOARD_PASSWORD=<board-password> REBOOT_AFTER_INSTALL=0 usb/install-kernel-image.sh <board-ip>`
3. `BOARD_PASSWORD=<board-password> REBOOT_AFTER_INSTALL=1 usb/install-usb-dtb.sh <board-ip>`
4. run the verification commands above

That is the working end-to-end procedure currently validated in this repository.

## USB Wi-Fi addendum for Realtek `0bda:c811`

### What this dongle is

The attached adapter identifies as:

- `idVendor=0bda`
- `idProduct=c811`
- interface class `ff/ff/ff`

That maps to the Realtek `RTL8821CU` / `RTL8811CU` family. Linux gained
in-tree support much later than 4.9, so this repository uses the external
`brektrou/rtl8821CU` driver for this board.

### Supported path on this board

The validated storage-only kernel does not export the wireless symbols this
driver needs, so use the Wi-Fi-capable kernel path:

If you want a kernel image with built-in wireless core support for follow-on
userspace work:

```sh
usb/build-usb-wifi-kernel.sh
BOARD_PASSWORD=root REBOOT_AFTER_INSTALL=0 \
  usb/install-kernel-image.sh <board-ip> usb/bin/Image-usb-storage-wifi
BOARD_PASSWORD=root REBOOT_AFTER_INSTALL=1 \
  usb/install-usb-dtb.sh <board-ip>
```

Then rebuild the module with cfg80211 support left enabled:

```sh
usb/build-rtl8821cu-module.sh
BOARD_PASSWORD=root usb/install-rtl8821cu.sh <board-ip>
```

After the board exposes `wlan0`, build and install Wi-Fi userspace:

```sh
usb/build-wifi-userspace.sh
BOARD_PASSWORD=root usb/install-wifi-userspace.sh <board-ip>
```

Then join a WPA/WPA2-PSK network and request DHCP:

```sh
BOARD_PASSWORD=root \
WIFI_SSID='your-ssid' \
WIFI_PSK='your-passphrase' \
usb/connect-wifi.sh <board-ip>
```

By default that host-side helper writes `/config/wpa_supplicant-wlan0.conf`,
sets it to mode `0600`, starts `/usr/local/bin/wpa_supplicant -D nl80211,wext`,
and runs BusyBox `udhcpc` on `wlan0` using interface-specific PID files so it
does not kill the board's existing `udhcpc` process for `eth0`.

When `eth0` and `wlan0` are both on the same IPv4 subnet, the helper also
installs a source-based policy route for the Wi-Fi address so replies sourced
from `wlan0` do not get sent back out `eth0`.

If you prefer to keep credentials only on the board, create the config file
there once and then run `usb/connect-wifi.sh` without `WIFI_SSID` or
`WIFI_PSK`:

```sh
ssh root@<board-ip>
export PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin
export LD_LIBRARY_PATH=/lib

IFACE=wlan0
CONF=/config/wpa_supplicant-$IFACE.conf

umask 077
cat > "$CONF" <<'EOF'
ctrl_interface=/var/run/wpa_supplicant
update_config=1
network={
  ssid="your-ssid"
  psk="your-passphrase"
  key_mgmt=WPA-PSK
}
EOF
chmod 600 "$CONF"
```

Then connect using the existing on-device config:

```sh
BOARD_PASSWORD=root usb/connect-wifi.sh <board-ip>
```

You can also point the helper at a different on-device file:

```sh
BOARD_PASSWORD=root \
WIFI_CONFIG_PATH=/etc/wpa_supplicant/wpa_supplicant.conf \
usb/connect-wifi.sh <board-ip>
```

### Automatic Wi-Fi At Boot

If you want the board to bring Wi-Fi up automatically at boot using the
existing `/config/wpa_supplicant-wlan0.conf`, install the autostart service:

```sh
BOARD_PASSWORD=root usb/install-wifi-autostart.sh <board-ip>
```

That installs `/etc/init.d/wifi-autostart` and enables it in runlevels
`2/3/4/5`. The installer also patches the LuxOS `networking` init script so
boot does not spend about a minute waiting for `eth0` DHCP when Ethernet has no
carrier and Wi-Fi autostart is configured. On boot it will:

- load `8821cu`
- start `wpa_supplicant` on `wlan0`
- run `udhcpc` on `wlan0`
- apply the source-based Wi-Fi routing policy
- remove the stale IPv4 from `eth0` when Ethernet has no carrier, so the old
  Ethernet address does not remain reachable over Wi-Fi

If you want to start it immediately after install instead of waiting for the
next reboot:

```sh
BOARD_PASSWORD=root START_AFTER_INSTALL=1 usb/install-wifi-autostart.sh <board-ip>
```

Optional overrides can be placed in `/config/wifi-autostart.conf`:

```sh
cat > /config/wifi-autostart.conf <<'EOF'
WIFI_IFACE=wlan0
WIFI_CONFIG_PATH=/config/wpa_supplicant-wlan0.conf
WIFI_ROUTE_TABLE=101
ETH_IFACE=eth0
EOF
chmod 600 /config/wifi-autostart.conf
```

### Notes

- The driver source includes USB IDs for both `0bda:c811` and `0bda:c82b`.
- The driver embeds its own firmware image, so no extra firmware blob is
  required under `/lib/firmware` for this module path.
- The failed fast-path symptom on the storage-only kernel is:
  `8821cu: Unknown symbol wireless_send_event`.
- The board already ships `ifconfig`, `ip`, and `udhcpc`, so the missing
  userspace piece was WPA itself rather than DHCP or basic interface control.
- The board does not ship the `libnl` runtime needed for `nl80211`, so the
  userspace build exports and installs the matching armhf `libnl` shared
  libraries alongside `wpa_supplicant`.
- Some `RTL8821CU` adapters need `usb_modeswitch -K -v 0bda -p c811` before the
  driver binds. This repository does not currently ship `usb_modeswitch`, so
  only add that step if the module loads but no wireless interface appears.