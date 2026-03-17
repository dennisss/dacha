This is a service which sets GPIO17 high when the `multi-user` systemd target is reached. You can install this on an SDCard mounted to the local computer as follows:

```
ROOTFS_DIR=/media/dennis/rootfs1

sudo cp pkg/rpi/gpio_boot_signal/gpio-boot-signal.service $ROOTFS_DIR/etc/systemd/system/gpio-boot-signal.service

sudo ln -s /etc/systemd/system/gpio-boot-signal.service $ROOTFS_DIR/etc/systemd/system/multi-user.target.wants/gpio-boot-signal.service

```