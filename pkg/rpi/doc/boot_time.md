# Raspberry Pi Boot Time

This page documents how to measure and improve Raspberry Pi boot times. If you are using the recommended image and installation [instructions](/pkg/rpi/index.md), then most of these optimizations should already be applied but you will need to manually apply the [bootloader/eeprom](#bootloader) config changes once you have a bootable system.

## Current Best Performance

- Raspberry Pi 4B : 8GB
    - ~9.5 seconds from power to SSH
- Raspberry Pi 5 : 8GB
    - ~8.9 seconds from power to SSH
        - systemd-analyze says 2.1 seconds (kernel + userspace)
- Raspberry Pi CM5 2GB RAM / 16GB eMMC
    - ~8 seconds from power to SSH


## Measurement Methods

Note that all measurements should be taken on the second boot and afterwards (since the first boot typically is used for expanding the SDCard, generating SSH keys, etc.).

**E2E time**

Build a microcontroller board with a MOSFET to turn on the Pi and at least one GPIO input to monitor when the Pi is done booting.

- Currently I use this [board](/pkg/cluster/machines/jbod/boards/backplane-tester/) which I already made for my JBOD project
  - Connect 5V/GND to the test board
  - Pi power connected to IN_V2 and IN_GND2 pins.
  - GPIO17 from the Pi is connected to SAS_V2 input on the tester board.
- Configure the Pi after SDCard flashing with [this service](../gpio_boot_signal/index.md).
- Connect the test board to your computer and run a test using:
  - `cargo run --bin jbod_tester -- test-boot-time`
  - The total boot time will be printed to the CLI.

**Measuring time after the Linux Kernel starts**

- `systemd-analyze`
- `systemd-analyze blame`
- `systemd-analyze critical-chain`

Note that these tools don't measure time spent in the bootloader or PMIC

**Bootloader Console**

- Hook up a serial to the bootloader UART (dedicated port on Pi 5 or UART 14/15 on Pi 4)
- Enable bootloader stage 1 UART:
  - run `sudo rpi-eeprom-config --edit`
  - set `BOOT_UART=1`
  - note that this will make bootup slower though.

- Enable bootloader stage 2 logging by adding the following to `/boot/firmware/config.txt`

```
uart_2ndstage=1
enable_uart=1
```

- You can also see part of the bootloader log via the following linux command (only stage 2 logs)
  - `sudo vclog -m`

## Baseline Measurements

### Raspberry Pi 4B 8GB : Raspberry Pi OS Lite (2025-12-04)

**E2E Time**: 28.062393999s

Systemd reported times:

```
$ systemd-analyze
Startup finished in 4.083s (kernel) + 15.491s (userspace) = 19.574s 
multi-user.target reached after 11.018s in userspace.

$ systemd-analyze blame
4.518s NetworkManager.service
4.277s NetworkManager-wait-online.service
3.552s apt-daily-upgrade.service
2.746s cloud-init-main.service
1.749s dev-mmcblk0p2.device
1.207s e2scrub_reap.service
 650ms ModemManager.service
 635ms rpi-eeprom-update.service
 505ms user@1000.service
 483ms systemd-fsck@dev-disk-by\x2dpartuuid-27de8f3b\x2d01.service
 417ms cloud-init-local.service
 400ms polkit.service
 378ms dev-mqueue.mount
 377ms run-lock.mount
 369ms cloud-init-network.service
 368ms sys-kernel-tracing.mount
 356ms keyboard-setup.service
 345ms sys-kernel-debug.mount
 343ms kmod-static-nodes.service
 340ms rpi-resize-swap-file.service
 334ms avahi-daemon.service
 330ms modprobe@configfs.service
 325ms bluetooth.service
 316ms modprobe@drm.service
 293ms dbus.service
 290ms modprobe@fuse.service
 273ms systemd-udev-trigger.service
 259ms systemd-binfmt.service

$ systemd-analyze critical-chain
multi-user.target @11.018s
└─ssh.service @10.829s +187ms
  └─network.target @10.818s
    └─NetworkManager.service @6.297s +4.518s
      └─dbus.service @5.995s +293ms
        └─basic.target @5.975s
          └─sockets.target @5.975s
            └─systemd-hostnamed.socket @5.975s
              └─sysinit.target @5.968s
                └─cloud-init-network.service @5.597s +369ms
                  └─cloud-init-local.service @5.177s +417ms
                    └─cloud-init-main.service @2.429s +2.746s
                      └─systemd-remount-fs.service @2.216s +180ms
                        └─systemd-journald.socket @1.841s
                          └─-.mount @1.542s
                            └─-.slice @1.542s
```

Bootloader UART:

```
 0.79 RPi: BOOTSYS release VERSION:d76c4603 DATE: 2026/01/09 TIME: 16:12:13
  0.79 BOOTMODE: 0x06 partition 0 build-ts BUILD_TIMESTAMP=1767975133 serial dda67451 boardrev d03115 stc 796145
  0.80 PM_RSTS 00000020
  0.80 POWER_OFF_ON_HALT: 0 WAIT_FOR_POWER_BUTTON 0 power-on-reset 0
  0.81 part 00000000 reset_info 00000000
  0.82 uSD voltage 3.3V
  0.84 Initialising SDRAM rank 2 total-size: 64Gbit 3200 part: 0 (0x14 0x00)
  0.85 DDR 3200 1 0 64 152 BL:3
  2.27 SD_SB 11013130
  2.44 OTP boardrev d03115 bootrom 8b0 8b0
  2.44 Customer key hash 0000000000000000000000000000000000000000000000000000000000000000
  2.45 VC-JTAG unlocked


  2.74 RPi: BOOTLOADER release VERSION:d76c4603 DATE: 2026/01/09 TIME: 16:12:13
  2.75 BOOTMODE: 0x06 partition 0 build-ts BUILD_TIMESTAMP=1767975133 serial dda67451 boardrev d03115 stc 2753469
  2.84 SD_OC: 0
  2.48 Boot mode: SD (01) order f
  2.86 SD HOST: 200000000 CTL0: 0x00800000 BUS: 400000 Hz actual: 390625 HZ div: 512 (256) status: 0x1fff0000 delay: 276
  2.87 SD HOST: 200000000 CTL0: 0x00800f00 BUS: 400000 Hz actual: 390625 HZ div: 512 (256) status: 0x1fff0000 delay: 276
  3.17 OCR c0ff8000 [376]
CID: 001b534d454332515430591053230135
CSD: 400e00325b590001dcff7f800a404000
  3.18 SD: bus-width: 4 spec: 2 SCR: 0x02c58003 0x00000000
  3.19 SD HOST: 200000000 CTL0: 0x00800f04 BUS: 50000000 Hz actual: 50000000 HZ div: 4 (2) status: 0x1fff0000 delay: 2
  3.20 MBR: 0x00004000, 1048576 type: 0x0c
  3.20 MBR: 0x00104000,123977728 type: 0x83
  3.20 MBR: 0x00000000,       0 type: 0x00
  3.21 MBR: 0x00000000,       0 type: 0x00
  3.15 Trying partition: 0
  3.18 type: 32 lba: 16384 'mkfs.fat' ' bootfs     ' clusters 261116 (4)
  3.22 rsc 32 fat-sectors 2040 root dir cluster 2 sectors 0 entries 0
  3.23 FAT32 clusters 261116
  3.24 [sdcard] autoboot.txt not found
  3.24 Select partition rsts 0 C(boot_partition) 0 EEPROM config 0 result 1
  3.47 Trying partition: 1
  3.50 type: 32 lba: 16384 'mkfs.fat' ' bootfs     ' clusters 261115 (4)
  3.25 rsc 32 fat-sectors 2040 root dir cluster 2 sectors 0 entries 0
  3.26 FAT32 clusters 261115
  3.68 Read config.txt bytes     1495 hnd 0x2fb74
  3.27 pieeprom.sig
  3.27 hash: 8d592b03978542d9bfd2a6b85aa1a76b32e2c0d26d5801fdc56b927ef98cd431
  3.27 ts: 1770689871
  3.68 SELF-UPDATE timestamp current 1770689871 new 1770689871 skip
  3.37 [sdcard] recover4.elf not found
  3.37 [sdcard] recovery.elf not found
  3.04 Read start4.elf bytes  2299008 hnd 0x6500
  3.08 Read fixup4.dat bytes     5496 hnd 0x257
  3.10 0x00d03115 0x00000000 0x00001fff
  3.13 MEM GPU: 76 ARM: 948 TOTAL: 1024
  3.80 Firmware: cd866525580337c0aee4b25880e1f5f9f674fb24 Aug 20 2025 17:02:31
  3.17 Starting start4.elf @ 0xfec00200 partition 1
  3.61 PCI0 reset
  3.63 +

MESS:00:00:03.730367:0: arasan: arasan_emmc_open
MESS:00:00:03.732029:0: arasan: arasan_emmc_set_clock C0: 0x00800000 C1: 0x000e0047 emmc: 200000000 actual: 390625 div: 0x00000100 target: 400000 min: 400000 max: 400000 delay: 5
MESS:00:00:03.852269:0: arasan: arasan_emmc_set_clock C0: 0x00800000 C1: 0x000e0047 emmc: 200000000 actual: 390625 div: 0x00000100 target: 400000 min: 400000 max: 400000 delay: 5
MESS:00:00:03.865140:0: arasan: arasan_emmc_set_clock C0: 0x00800f00 C1: 0x000e0047 emmc: 200000000 actual: 390625 div: 0x00000100 target: 400000 min: 390000 max: 400000 delay: 5
MESS:00:00:03.898721:0: boot-part: 1 fs-type: 0
MESS:00:00:03.900133:0: boot-part: 1 fs-type: 3
MESS:00:00:03.904511:0: arasan: arasan_emmc_set_clock C0: 0x00800f06 C1: 0x000e0207 emmc: 200000000 actual: 50000000 div: 0x00000002 target: 50000000 min: 0 max: 50000000 delay: 1
MESS:00:00:04.075859:0: brfs: File read: /mfs/sd/config.txt
MESS:00:00:04.079211:0: brfs: File read: 1495 bytes
MESS:00:00:04.106514:0: HDMI1:EDID error reading EDID block 0 attempt 0
MESS:00:00:04.111023:0: HDMI1:EDID giving up on reading EDID block 0
MESS:00:00:04.121715:0: brfs: File read: /mfs/sd/config.txt
MESS:00:00:04.615798:0: gpioman: gpioman_get_pin_num: pin DISPLAY_DSI_PORT not defined
MESS:00:00:04.621353:0: gpioman: gpioman_get_pin_num: pin DISPLAY_DSI_PORT not defined
MESS:00:00:04.638757:0: *** Restart logging
MESS:00:00:04.639837:0: brfs: File read: 1495 bytes
MESS:00:00:04.646325:0: hdmi: HDMI:hdmi_get_state is deprecated, use hdmi_get_display_state instead
MESS:00:00:04.658246:0: hdmi: HDMI1:EDID error reading EDID block 0 attempt 0
MESS:00:00:04.663268:0: hdmi: HDMI1:EDID giving up on reading EDID block 0
MESS:00:00:04.673891:0: hdmi: HDMI1:EDID error reading EDID block 0 attempt 0
MESS:00:00:04.678920:0: hdmi: HDMI1:EDID giving up on reading EDID block 0
MESS:00:00:04.684518:0: hdmi: HDMI:hdmi_get_state is deprecated, use hdmi_get_display_state instead
MESS:00:00:04.693282:0: HDMI0: hdmi_pixel_encoding: 300000000
MESS:00:00:04.698756:0: HDMI1: hdmi_pixel_encoding: 300000000
MESS:00:00:05.622476:0: brfs: File read: /mfs/sd/initramfs8
MESS:00:00:05.624940:0: Loaded 'initramfs8' to 0x0 size 0xa5a68a
MESS:00:00:05.639127:0: initramfs (initramfs8) loaded to 0x2e5a5000 (size 0xa5a68a)
MESS:00:00:05.643699:0: dtb_file 'bcm2711-rpi-4-b.dtb'
MESS:00:00:05.648537:0: brfs: File read: 10856074 bytes
MESS:00:00:05.660211:0: brfs: File read: /mfs/sd/bcm2711-rpi-4-b.dtb
MESS:00:00:05.663456:0: Loaded 'bcm2711-rpi-4-b.dtb' to 0x100 size 0xdbb9
MESS:00:00:05.683823:0: brfs: File read: 56249 bytes
MESS:00:00:05.700952:0: brfs: File read: /mfs/sd/overlays/overlay_map.dtb
MESS:00:00:05.729514:0: brfs: File read: 5971 bytes
MESS:00:00:05.734059:0: brfs: File read: /mfs/sd/config.txt
MESS:00:00:05.736682:0: dtparam: sd_force_pio=1
MESS:00:00:05.749336:0: dtparam: audio=off
MESS:00:00:05.755614:0: brfs: File read: 1495 bytes
MESS:00:00:05.778951:0: brfs: File read: /mfs/sd/overlays/vc4-kms-v3d-pi4.dtbo
MESS:00:00:05.846037:0: Loaded overlay 'vc4-kms-v3d-pi4'
MESS:00:00:06.000574:0: brfs: File read: 3913 bytes
MESS:00:00:06.007225:0: brfs: File read: /mfs/sd/overlays/disable-bt.dtbo
MESS:00:00:06.025998:0: Loaded overlay 'disable-bt'
MESS:00:00:06.061488:0: brfs: File read: 1073 bytes
MESS:00:00:06.067907:0: brfs: File read: /mfs/sd/overlays/disable-wifi.dtbo
MESS:00:00:06.080627:0: Loaded overlay 'disable-wifi'
MESS:00:00:06.114042:0: brfs: File read: 387 bytes
MESS:00:00:06.117995:0: brfs: File read: /mfs/sd/cmdline.txt
MESS:00:00:06.121128:0: Read command line from file 'cmdline.txt':
MESS:00:00:06.127003:0: 'root=PARTUUID=a2ac7dd0-02 rootfstype=ext4 fsck.repair=yes rootwait cfg80211.ieee80211_regdom=US quiet loglevel=3 fastboot'
MESS:00:00:06.264303:0: brfs: File read: 122 bytes
MESS:00:00:07.084633:0: brfs: File read: /mfs/sd/kernel8.img
MESS:00:00:07.087191:0: Loaded 'kernel8.img' to 0x200000 size 0x93d213
MESS:00:00:08.389676:0: Device tree loaded to 0x2e596f00 (size 0xe0ac)
MESS:00:00:08.396261:0: uart: Set PL011 baud rate to 103448.300000 Hz
MESS:00:00:08.402500:0: uart: Baud rate change done...
MESS:00:00:08.404522:0: uart: Baud rate change done...
MESS:00:00:08.412252:0: gpioman: gpioman_get_pin_num: pin SDCARD_CONTROL_POWER not defined
MESS:00:00:08.417559:0: Watchdog stopped
MESS:00:00:08.421056:0: arm_loader: Starting ARM with 948MB
/scripts/init-top/rpi_wd: 14: /scripts/init-top/rpi_wd: grep: not found
/scripts/local-premount/resize_early: 11: /scripts/local-premount/resize_early: grep: not found
/scripts/local-bottom/imager_fixup: 13: /scripts/local-bottom/imager_fixup: grep: not found
/scripts/local-bottom/set_partuuid: 14: /scripts/local-bottom/set_partuuid: grep: not found
                                                                                                                                                                  ^[[37;1R^[[37;163R                                                                                                                                                 ^[[37;18R^[[37;163R
Debian GNU/Linux 13 testpi ttyAMA0

```


### Raspberry Pi 5 8GB : Raspberry Pi OS Lite (2025-12-04)

**E2E Time**: 19.253104126s

Systemd reported times:

```
$ systemd-analyze 
Startup finished in 3.539s (kernel) + 10.175s (userspace) = 13.714s 
multi-user.target reached after 5.844s in userspace.

$ systemd-analyze blame
4.332s NetworkManager-wait-online.service
2.076s NetworkManager.service
1.464s cloud-init-main.service
1.201s dev-mmcblk0p2.device
 637ms e2scrub_reap.service
 400ms ModemManager.service
 277ms rpi-eeprom-update.service
 272ms dev-mqueue.mount
 250ms keyboard-setup.service
 241ms kmod-static-nodes.service
 236ms run-lock.mount
 236ms sys-kernel-debug.mount
 235ms sys-kernel-tracing.mount
 233ms systemd-fsck@dev-disk-by\x2dpartuuid-27de8f3b\x2d01.service
 232ms modprobe@configfs.service
 225ms modprobe@drm.service
 220ms avahi-daemon.service
 218ms bluetooth.service
 206ms polkit.service
 205ms modprobe@fuse.service
 187ms systemd-binfmt.service
 185ms dbus.service
 184ms user@1000.service
 174ms rpi-resize-swap-file.service
 163ms cloud-init-local.service

$ systemd-analyze critical-chain
multi-user.target @5.844s
└─gpio-boot-signal.service @5.828s +15ms
  └─ssh.service @5.685s +141ms
    └─network.target @5.681s
      └─NetworkManager.service @3.605s +2.076s
        └─dbus.service @3.416s +185ms
          └─basic.target @3.387s
            └─sockets.target @3.387s
              └─systemd-hostnamed.socket @3.387s
                └─sysinit.target @3.384s
                  └─cloud-init-network.service @3.062s +154ms
                    └─cloud-init-local.service @2.897s +163ms
                      └─cloud-init-main.service @1.432s +1.464s
                        └─systemd-remount-fs.service @1.303s +117ms
                          └─systemd-journald.socket @1.024s
                            └─system.slice @817ms
                              └─-.slice @817ms
```


### Raspberry Pi 4B 8GB : Mainsail 

**E2E Time**:
- SSH Ready: 19.39729917s
- Klipper Socket Ready: 26.306366839s

Systemd reported times:

```
$ systemd-analyze 
Startup finished in 5.581s (kernel) + 10.297s (userspace) = 15.878s 
multi-user.target reached after 10.270s in userspace.

$ systemd-analyze
6.337s NetworkManager-wait-online.service
 820ms dev-mmcblk0p2.device
 798ms ModemManager.service
 665ms NetworkManager.service
 642ms systemd-networkd.service
 464ms rpi-eeprom-update.service
 422ms avahi-daemon.service
 417ms systemd-fsck@dev-disk-by\x2dpartuuid-60c7dd71\x2d01.service
 407ms polkit.service
 405ms e2scrub_reap.service
 391ms systemd-logind.service
 371ms dbus.service
 368ms user@1000.service
 330ms ssh.service
 324ms headless_nm.service
 281ms dphys-swapfile.service
 262ms sshswitch.service
 244ms keyboard-setup.service
 238ms nginx.service
 221ms wpa_supplicant.service
 208ms systemd-udev-trigger.service
 191ms packagekit.service
 160ms systemd-udevd.service
 133ms systemd-rfkill.service
 114ms fake-hwclock.service
 114ms systemd-journald.service
 105ms triggerhappy.service
 104ms modprobe@drm.service

$ systemd-analyze critical-chain

multi-user.target @10.270s
└─nginx.service @10.026s +238ms
  └─network-online.target @9.954s
    └─NetworkManager-wait-online.service @3.615s +6.337s
      └─NetworkManager.service @2.931s +665ms
        └─dbus.service @2.523s +371ms
          └─basic.target @2.480s
            └─sockets.target @2.480s
              └─triggerhappy.socket @2.480s
                └─sysinit.target @2.476s
                  └─systemd-timesyncd.service @2.383s +91ms
                    └─systemd-tmpfiles-setup.service @2.317s +58ms
                      └─local-fs.target @2.301s
                        └─run-credentials-systemd\x2dtmpfiles\x2dsetup.service.mount @2.324s
                          └─local-fs-pre.target @941ms
                            └─systemd-tmpfiles-setup-dev.service @907ms +33ms
                              └─systemd-sysusers.service @800ms +81ms
                                └─systemd-remount-fs.service @705ms +80ms
                                  └─systemd-journald.socket @648ms
                                    └─system.slice @601ms
                                      └─-.slice @601ms

```

## Optimizations

### Early Boot Optimizations

Add to the end of `/boot/firmware/config.txt` (in the `[all]` section)

```
initial_turbo=30
boot_delay=0
disable_splash=1
bootcode_delay=0
force_eeprom_read=0

# Cutdown firmware
start_cd=1

camera_auto_detect=0
display_auto_detect=0
```

**Impact**: Not sure. Boot-to-boot time variance is higher.

Note that the cutdown firmware disables boot logging so not generally good.

TODO: Re-test once more stabilized.

### [Bootloader](#bootloader)

Run `sudo rpi-eeprom-config --edit` and edit the file to make sure that the following variables are set:

```
BOOT_UART=0
BOOT_ORDER=0xf1
DISABLE_HDMI=1
NET_INSTALL_ENABLED=0
```

Note that `BOOT_UART` defualts to 1 on RPI 5 so needs to be overriden.

A example of a complete config for the Pi 5 / 4B is show below:

```
[all]
BOOT_UART=0
WAKE_ON_GPIO=1
POWER_OFF_ON_HALT=0
BOOT_ORDER=0xf1
DISABLE_HDMI=1
NET_INSTALL_ENABLED=0
```

For a CM5, a good config is:

```
[all]
BOOT_UART=0
POWER_OFF_ON_HALT=1
BOOT_ORDER=0xf1
DISABLE_HDMI=1
NET_INSTALL_ENABLED=0
```

For compute modules this will boot from eMMC or SDCard depending on whether or not you have the lite or regular version of the CM.

**Flags explanation:**

- `BOOT_ORDER=0xf1` only try the SDCard (or eMMC) when booting (no USB / Net / etc. boot).
- `BOOT_UART=0` disables bootloader logging.
- The bootloader initially waits for 900ms (defined by the `NET_INSTALL_KEYBOARD_WAIT` variable) for a keyboard and 'shift' key press to be present to determine if network install mode should be entered. Both `DISABLE_HDMI=1` and `NET_INSTALL_ENABLED=0` force disabling NET_INSTALL so that has the biggest impact on speeding on boot time.

**Impact**: ~0.9 seconds

Documentation on all Bootloader Options:

- https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#configuration-properties


### Quiet Boot

Adding the following to `/boot/firmware/cmdline.txt`:

```
quiet loglevel=3 fastboot
```

**Impact**: ~2-3 seconds on both Pi 4 and 5


### Get Rid of Network Manager

Network Manager is really heavy and generally unnecessary for headless applications. Instead we will mainly use `systemd-networkd` (and `wpa_supplicant` if wireless is needed):

Add the following to `/etc/systemd/network/10-eth0.network` to configure the ethernet port to connect via DHCP:

```
[Match]
Name=eth0

[Network]
DHCP=yes
```

Alternatively you can configure a static IP with something like this (to further speed up boot):

```
[Match]
Name=eth0

[Network]
Address=10.1.1.9/16
Gateway=10.1.0.1
DNS=10.1.0.1
```


Then get rid of Network Manager:

```
sudo apt purge network-manager -y
sudo apt purge modemmanager -y
sudo apt autoremove -y
```

Finally before restarting make sure that `systemd-networkd` is enabled:

```
sudo systemctl enable systemd-networkd
```

**Impact:** For an ethernet setup on a Pi 4 with DHCP, ~4 seconds faster.


### Get Rid of Cloud Init

If you don't know what this is, you probably don't need it.

```
sudo apt purge cloud-init rpi-cloud-init-mods -y
sudo apt autoremove -y

sudo rm /boot/firmware/meta-data
sudo rm /boot/firmware/user-data 
sudo rm /boot/firmware/network-config
```

**Impact:** ~2 seconds boot time reduction on Pi 4


### Get Rid Of Binfmt

```
sudo systemctl mask systemd-binfmt.service
```

**Impact:** ~200ms

### Disable Wireless

If you aren't using wireless, you can disable it as follows:

Run:

```
sudo systemctl disable wpa_supplicant.service
```

Also add to `/boot/firmware/config.txt`:

```
dtoverlay=disable-bt
dtoverlay=disable-wifi
```

**Impact:** ~1 second


### Disable EEPROM Updater

This will make boot times unstable unless disabled since it contacts the network and may require EEPROM flashing:

```
sudo systemctl mask rpi-eeprom-update
```

**Impact:** ~0.5 seconds


### Disable fsck on boot

Modify `/etc/fstab` and set the last field in each row to `0`.

**Impact:** ~100-200ms

### Disable SWAP

```
sudo apt purge rpi-swap systemd-zram-generator -y
sudo apt autoremove -y
```

**Impact:** Maybe ~100ms


### Misc Service Cleanup

```

sudo systemctl mask sshswitch.service
sudo systemctl mask keyboard-setup.service
sudo systemctl mask avahi-daemon.socket
sudo systemctl mask avahi-daemon.service
sudo systemctl mask e2scrub_reap.service
sudo systemctl mask e2scrub_all.timer

# TODO: Eventually get rid of everything in /etc/systemd/system/timers.target.wants
sudo systemctl mask apt-daily.timer
sudo systemctl mask apt-daily-upgrade.timer
sudo systemctl mask dpkg-db-backup.timer
```

**Impact**: ~500ms


### Speed Up Initramfs

#### Option 1: Disable Initramfs

Set `auto_initramfs=0` in `/boot/firmware/config.txt`.

#### Option 2: Optimize Initramfs

The goal here is to minimize the size of the `/boot/firmware/initramfs*` files so that less stuff is loaded before the kernel starts running (this is to reduce the dead time not measured by `systemd-analyze`).

You can check the baseline size of these files as follows:

```
ls -al /boot/firmware/initramfs*
-rwxr-xr-x 1 root root 16008939 Dec  4 09:46 /boot/firmware/initramfs_2712
-rwxr-xr-x 1 root root 16015335 Dec  4 09:46 /boot/firmware/initramfs8
```

How this is built is configured in `/etc/initramfs-tools/initramfs.conf`. This file should already have `MODULES=dep` in it which isn't used when the sdcard image was built so if you just run `sudo update-initramfs -u` now, you will already see improvements.

But we can get further improvements by modifying `/etc/initramfs-tools/initramfs.conf` to include (modify the existing variable lines):

```
BUSYBOX=n
COMPRESSLEVEL=12
```

Then run `sudo update-initramfs -u`

Now the files should be much smaller:

```
ls -al /boot/firmware/initramfs*
-rwxr-xr-x 1 root root 10855739 Feb  9 19:53 /boot/firmware/initramfs_2712
-rwxr-xr-x 1 root root 10856074 Feb  9 19:53 /boot/firmware/initramfs8
```

**Impact:** ~1 second


## Aggressive Boot Partition Optimization

Explicitly specify the kernel and device_tree file to use in config.txt. Here is an example for the Pi 4:

```
kernel=kernel8.img
device_tree=bcm2711-rpi-4-b.dtb
```

The operating theory is that reduces the amount of SDCard I/Os that are needed so will generally speed things up.

**Impact:** ~1 second speed up.

## Journald

Disk writes are slow and also wear down the SDCard quickly so it is recommended to switch the logging to just store all logs in memory rather than persisting them to disk:

```
sudo sed -i 's/^#*Storage=.*/Storage=volatile/' /etc/systemd/journald.conf
sudo sed -i 's/^#*RuntimeMaxUse=.*/RuntimeMaxUse=50M/' /etc/systemd/journald.conf
```

## Future Improvements

Things already integrated into the custom image but not benchmarked yet:

- Kernel module is trimed of unused modules and RAID6 benchmarking on boot is disabled.
  - The kernel image is loaded early by the bootloader so having a smaller image should have with boot speeds.
- Minimal config.txt file containing only necessary lines.
  - The rpi_imager makes one of these when specifying a '--hardware_model'. This similarly should minimize SDCard I/O and time in the bootloader

Other ideas for improving the boot time:

- Use less RAM
  - At boot, the bootloader needs to do RAM training which should be faster with less RAM.
  - This process might be cacheable but unfortunately the bootloader is proprietary so hard to edit.
- Embed dts / overlay files in the kernel image
  - Currently they are separate in the /boot/firmware filesystem so will require extra filesystem seeks to find and load.
- Make the /boot/firmware FAT filesystem more compact
  - The bootloader probably isn't doing advanced caching of the FAT filesystem so if there are many files to scan through in the file system, load times are likely to be slow.
  - Ideally we get rid of any unnecessary files and defragment the filesystem.

## References

- https://www.raspberrypi.com/documentation/computers/configuration.html#bootcode-bin
- https://github.com/IronOxidizer/instant-pi
- https://github.com/raspberrypi/firmware/issues/1375
- https://ohyaan.github.io/tips/raspberry_pi_boot_time_optimization__complete_performance_guide/#specialized-boot-configurations
- https://kittenlabs.de/blog/2024/09/01/extreme-pi-boot-optimization/
  - KASLR
