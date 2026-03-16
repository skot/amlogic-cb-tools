# USB Enablement Procedure for Bitmain Amlogic Controlboards

This directory collects the validated procedure for enabling Linux USB
host and USB mass storage on the Bitmain Amlogic controlboard used in this
repository.

The currently validated working combination is:

- kernel image: `usb/Image-usb-storage`
- DTB: `usb/axg_s400_antminer.usb-host-nand-clocks.dtb`

With that pair installed on a compatible board, the board:

- boots normally from UBIFS root on `ubi0:nvdada_log`
- initializes `mtd6` correctly
- binds `usb-storage`
- exposes `/dev/sda` and `/dev/sda1`
- passes live USB read/write testing

## What is in this directory

- `build-usb-storage-kernel.sh`
  Wrapper for the validated Docker-based kernel build helper.
- `install-kernel-image.sh`
  Wrapper for the guarded kernel image installer.
- `install-usb-dtb.sh`
  Wrapper for the guarded DTB installer.

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

Everything needed for the workflow is in `usb/`:

- extracted vendor baseline config:
  `usb/Antminer-4.9.241.config`
- validated rebuilt kernel:
  `usb/Image-usb-storage`
- kernel checksum:
  `usb/Image-usb-storage.sha256`
- validated DTB for rebuilt kernel:
  `usb/axg_s400_antminer.usb-host-nand-clocks.dtb`
- DTB checksum:
  `usb/axg_s400_antminer.usb-host-nand-clocks.dtb.sha256`

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
  `usb/Image-usb-storage`
- kernel checksum:
  `usb/Image-usb-storage.sha256`
- final config:
  `usb/.config.final`
- logs:
  `usb/olddefconfig.log`
  `usb/build.log`

## Step 2: Install the rebuilt kernel on a live board

From the repository root:

```sh
BOARD_PASSWORD=root REBOOT_AFTER_INSTALL=0 \
  usb/install-kernel-image.sh <board-ip>
```

What it does:

- verifies the local image checksum
- backs up `/Image` to `/Image.pre-usb-storage`
- uploads the rebuilt image through plain SSH
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
usb/axg_s400_antminer.usb-host-nand-clocks.dtb
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
- DTB checksum matches `usb/axg_s400_antminer.usb-host-nand-clocks.dtb`

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