Setup CPU Slot 7 in x4x4x4x4 bifurcation in the BIOS.

Install Ubuntu Server 22.04:
- Minimal Install
- Custom storage layout
  - 1GB EFI partition ('/boot/efi')
  - Rest of disk as BTRFS '/' partition
- Install OpenSSH

Install Nvidia drivers as described in https://docs.nvidia.com/datacenter/tesla/tesla-installation-notes/index.html

```
sudo apt-get install linux-headers-$(uname -r)
distribution=$(. /etc/os-release;echo $ID$VERSION_ID | sed -e 's/\.//g')
wget https://developer.download.nvidia.com/compute/cuda/repos/$distribution/x86_64/cuda-keyring_1.0-1_all.deb
sudo dpkg -i cuda-keyring_1.0-1_all.deb
sudo apt-get -y install cuda-drivers
```

Next install CUDA toolkit to be able to compile apps: https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html#abstract

```
sudo apt-get -y install cuda
```


Other useful utilities:

```
sudo apt install vim smartmontools hdparm ipmitool lm-sensors
```



```
sudo apt install hdparm

# Use to instantly set a disk into standby mode
sudo hdparm -y /dev/sda

sudo vim /etc/hdparm.conf
```

Add the following to the file to make all 6 drives spin down after 30 minutes of inactivity:

```
/dev/sda {
    spindown_time = 241
}
/dev/sdb {
    spindown_time = 241
}
/dev/sdc {
    spindown_time = 241
}
/dev/sdd {
    spindown_time = 241
}
/dev/sde {
    spindown_time = 241
}
/dev/sdf {
    spindown_time = 241
}
```

Based on information in https://forums.servethehome.com/index.php?resources/supermicro-x9-x10-x11-fan-speed-control.20/ we can control the fan speed:

```
sudo ipmitool raw 0x30 0x70 0x66 0x01 0x00 0x64
```

where the last byte ranges from 0 to 0x64 (100%)



nvme0n1
nvme1n1
nvme2n1


ubuntu-drivers devices

CPU Slot 7 has NVME



NF-A12x25 is:
- 102,1 m^3/h (60.09 CFM)


For connecting additional NVME:
- SFF-8654

## Fan Shroud

40.64mm per 2 Slot GPU

- NF-A8 PWM
  - 55.5 m^3/h
- NF-A4x20 PWM
  - 9.4 m^3/h

23mm from bottom of card to bottom of 


The GPU has an 8-pin EPS +12V connector
- References
  - https://allpinouts.org/pinouts/connectors/power_supply/eps12v-eatx12v-8-pin/
  - https://www.moddiy.com/pages/Power-Supply-Connectors-and-Pinouts.html
- Motherboard Connector Molex 39-28-1083
  - 'Mini-Fit Jr. 4.20mm Pitch'
  - 'Mini-Fit Jr 5566'
- Cable Connector: Molex 39-01-2080
  - According to molex, mates with series: 5559, 5566, 5569, 
- Terminals: Molex 39-00-0168, Molex 44476-1111

Other compatible connectos:
- Female Right Angle
  - 39301080
  - Alternative black:  469990016


Internal USB connector
- 20-pin (2x10) (19 pins used)
- 2mm pitch
- Something like G823J201240BHR (0.4mm thick contacts)

## Stress Testing


```
sudo apt install git build-essential
```

