# Radxa CM3

Optimizing boot time: https://industrialmonitordirect.com/blogs/knowledgebase/rk3566-linux-sub-2-second-boot-optimization-guide?srsltid=AfmBOoqlP8IrkU8vP0uBHTyndp-mpXfNlQnHwCqeFdfis061xHFHlsjH

Download Armbian image:
- https://armbian.com/boards/rock-3a
- (minimal)

Extract the `.img`.

Then modify it:

- `sudo losetup -Pf Armbian_*.img`
- `sudo mkdir /mnt/radxa`
- `sudo mount /dev/loop0p1 /mnt/radxa`
    - Check `lsblk` to find the loop devide with a p1 partition
- Modify `/mnt/radxa/boot/armbianEnv.txt`
    - Set `fdtfile=rockchip/rk3566-radxa-cm3-io.dtb`
- `sudo umount /mnt`
- `sudo losetup -d /dev/loop0`

TODO: For SDCard support, we need to compile and switch to `rk3566-radxa-cm3-rpi-cm4-io.dtb`

Flash:

- tool: https://github.com/rockchip-linux/rkdeveloptool
- loader: https://dl.radxa.com/rock3/images/loader/rk356x_spl_loader_ddr1056_v1.06.110.bin
    - Linked from https://wiki.radxa.com/Rock3/installusb-install-radxa-cm3-rpi-cm4-io

```
./rkdeveloptool ld
./rkdeveloptool db rk356x_spl_loader_ddr1056_v1.06.110.bin
./rkdeveloptool wl 0 Armbian_*.img
./rkdeveloptool rd
```



- ``


Sign in via SSH

```
# initial password is '1234'
ssh root@10.1.0.140


ssh dennis@10.1.0.140

```


Ethernet interface is called `end0`


R8 Pinout (mapping from Pi to Radxa CM3):
    Pin 24 (Pi GPIO26)  -> GPIO0_C2
    Pin 26 (Pi GPIO19)  -> GPIO3_D0
    Pin 48 (Pi GPIO27)  -> GPIO0_B7
    Pin 50 (Pi GPIO17)  -> GPIO0_C7
    Pin 51 (Pi GPIO15) (UART TX) -> UART2
    Pin 55 (Pi GPIO14) (UART RX) -> UART2
    Pin 56 (Pi GPIO3) (SCL)      -> I2C2
    Pin 58 (Pi GPIO2) (SDA)      -> I2C2