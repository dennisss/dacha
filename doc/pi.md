
- https://feldspaten.org/2019/07/05/raspbian-and-btrfs/
- https://yagrebu.net/unix/rpi-overlay.md
- https://hackmd.io/@seaQueue/HJOFQphWW?type=view


Finding a raspberry pi:

```
sudo nmap -p 22 10.1.0.0/24
```

i2ctransfer 1 w1@0x30 0x82 r4


Expanding the partition:


```
sudo parted -s /dev/sdb "resizepart 2 -1" quit
sudo btrfs filesystem resize max /media/dennis/rootfs
```


Ideally I'd generate the host SSH keys at the time of flashing so that I know that I can trust them.


Camera

```
sudo apt install v4l-utils
```

- H264:
    - /dev/video11
    - https://github.com/raspberrypi/libcamera-apps/blob/main/encoder/h264_encoder.cpp



Requirements for a camera driver:
- Open camera
    - Configure basic settings like resolution, frame rate, exposure
- Request to get a frame
    - Block until frame received
- Close



Changing password just with an SDCard
    openssl passwd -6 -salt xyz password

    In /etc/shadow, set second field to the password
    cluster-user:<add-here>
