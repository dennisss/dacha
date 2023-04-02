# Notes from developing kernel modules

kbuild documentation:

- https://lwn.net/Articles/81398/
- https://www.kernel.org/doc/Documentation/kbuild/makefiles.txt


```


scp -i ~/.ssh/id_cluster -r third_party/raspberrypi cluster-user@10.1.0.112:~

ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.112

# note: uses raspberrypi-kernel-headers instead of linux-headers
sudo apt install build-essential linux-headers

cd raspberrypi

make all

sudo make install

dtc -@ -O dtb -o periphmem.dtbo periphmem.dts

sudo cp periphmem.dtbo /boot/overlays

echo 'SUBSYSTEM=="bcm2835-periphmem", GROUP="gpio", MODE="0660"' | sudo tee /etc/udev/rules.d/81-periphmem.rules 
```

Finally add `dtoverlay=periphmem` to the end of `/boot/config.txt`

Reboot and configure the permissions:

```
sudo chown root:gpio /dev/periphmem
sudo chmod 660 /dev/periphmem
```



Docs:
- https://tldp.org/LDP/lkmpg/2.6/html/lkmpg.html#AEN380
- https://olegkutkov.me/2018/03/14/simple-linux-character-device-driver/#:~:text=A%20character%20device%20is%20one,by%20byte%2C%20like%20a%20stream.


```
dtc -@ -O dtb -o periphmem.dtbo periphmem.dts
sudo insmod periphmem.ko
sudo dtoverlay periphmem.dtbo

sudo chown root:gpio /dev/periphmem
sudo chmod 660 /dev/periphmem
```



Overlay examples: https://github.com/raspberrypi/linux/tree/7f465f823c2ecbade5877b8bbcb2093a8060cb0e/arch/arm/boot/dts/overlays



