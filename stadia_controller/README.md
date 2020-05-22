


```
> sudo cat /sys/kernel/debug/usb/devices

T:  Bus=03 Lev=02 Prnt=02 Port=02 Cnt=03 Dev#= 10 Spd=480  MxCh= 0
D:  Ver= 2.01 Cls=ef(misc ) Sub=02 Prot=01 MxPS=64 #Cfgs=  1
P:  Vendor=18d1 ProdID=9400 Rev= 1.00
S:  Manufacturer=Google Inc.
S:  Product=Stadia Controller
S:  SerialNumber=9A050YCAC2CPTW
C:* #Ifs= 2 Cfg#= 1 Atr=80 MxPwr=500mA
A:  FirstIf#= 0 IfCount= 1 Cls=ff(vend.) Sub=00 Prot=00
A:  FirstIf#= 1 IfCount= 1 Cls=03(HID  ) Sub=00 Prot=00
I:* If#= 0 Alt= 0 #EPs= 2 Cls=ff(vend.) Sub=00 Prot=00 Driver=(none)
E:  Ad=87(I) Atr=02(Bulk) MxPS= 512 Ivl=0ms
E:  Ad=07(O) Atr=02(Bulk) MxPS= 512 Ivl=0ms
I:* If#= 1 Alt= 0 #EPs= 2 Cls=03(HID  ) Sub=00 Prot=00 Driver=usbhid
E:  Ad=83(I) Atr=03(Int.) MxPS=  64 Ivl=4ms
E:  Ad=03(O) Atr=03(Int.) MxPS=  64 Ivl=4ms
```

```
sudo usbhid-dump -d 18d1
003:010:001:DESCRIPTOR         1581782605.942688
 05 01 09 05 A1 01 85 03 05 01 75 04 95 01 25 07
 46 3B 01 65 14 09 39 81 42 45 00 65 00 75 01 95
 04 81 01 05 09 15 00 25 01 75 01 95 0F 09 12 09
 11 09 14 09 13 09 0D 09 0C 09 0B 09 0F 09 0E 09
 08 09 07 09 05 09 04 09 02 09 01 81 02 75 01 95
 01 81 01 05 01 15 01 26 FF 00 09 01 A1 00 09 30
 09 31 75 08 95 02 81 02 C0 09 01 A1 00 09 32 09
 35 75 08 95 02 81 02 C0 05 02 75 08 95 02 15 00
 26 FF 00 09 C5 09 C4 81 02 85 05 06 0F 00 09 97
 75 10 95 02 27 FF FF 00 00 91 02 C0

```

```
Bus 003 Device 013: ID 18d1:9400 Google Inc. 
Device Descriptor:
  bLength                18
  bDescriptorType         1
  bcdUSB               2.01
  bDeviceClass          239 Miscellaneous Device
  bDeviceSubClass         2 
  bDeviceProtocol         1 Interface Association
  bMaxPacketSize0        64
  idVendor           0x18d1 Google Inc.
  idProduct          0x9400 
  bcdDevice            1.00
  iManufacturer           1 Google Inc.
  iProduct                2 Stadia Controller
  iSerial                 3 9A050YCAC2CPTW
  bNumConfigurations      1
  Configuration Descriptor:
    bLength                 9
    bDescriptorType         2
    wTotalLength       0x0050
    bNumInterfaces          2
    bConfigurationValue     1
    iConfiguration          0 
    bmAttributes         0x80
      (Bus Powered)
    MaxPower              500mA
    Interface Association:
      bLength                 8
      bDescriptorType        11
      bFirstInterface         0
      bInterfaceCount         1
      bFunctionClass        255 Vendor Specific Class
      bFunctionSubClass       0 
      bFunctionProtocol       0 
      iFunction               0 
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        0
      bAlternateSetting       0
      bNumEndpoints           2
      bInterfaceClass       255 Vendor Specific Class
      bInterfaceSubClass      0 
      bInterfaceProtocol      0 
      iInterface              0 
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x87  EP 7 IN
        bmAttributes            2
          Transfer Type            Bulk
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0200  1x 512 bytes
        bInterval               0
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x07  EP 7 OUT
        bmAttributes            2
          Transfer Type            Bulk
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0200  1x 512 bytes
        bInterval               0
    Interface Association:
      bLength                 8
      bDescriptorType        11
      bFirstInterface         1
      bInterfaceCount         1
      bFunctionClass          3 Human Interface Device
      bFunctionSubClass       0 
      bFunctionProtocol       0 
      iFunction               0 
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        1
      bAlternateSetting       0
      bNumEndpoints           2
      bInterfaceClass         3 Human Interface Device
      bInterfaceSubClass      0 
      bInterfaceProtocol      0 
      iInterface              0 
        HID Device Descriptor:
          bLength                 9
          bDescriptorType        33
          bcdHID               1.11
          bCountryCode            0 Not supported
          bNumDescriptors         1
          bDescriptorType        34 Report
          wDescriptorLength     156
         Report Descriptors: 
           ** UNAVAILABLE **
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x83  EP 3 IN
        bmAttributes            3
          Transfer Type            Interrupt
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0040  1x 64 bytes
        bInterval               6
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x03  EP 3 OUT
        bmAttributes            3
          Transfer Type            Interrupt
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0040  1x 64 bytes
        bInterval               6
Binary Object Store Descriptor:
  bLength                 5
  bDescriptorType        15
  wTotalLength       0x0039
  bNumDeviceCaps          2
  Platform Device Capability:
    bLength                24
    bDescriptorType        16
    bDevCapabilityType      5
    bReserved               0
    PlatformCapabilityUUID    {3408b638-09a9-47a0-8bfd-a0768815b665}
      WebUSB:
        bcdVersion    1.00
        bVendorCode      1
        iLandingPage     0 
  Platform Device Capability:
    bLength                28
    bDescriptorType        16
    bDevCapabilityType      5
    bReserved               0
    PlatformCapabilityUUID    {d8dd60df-4589-4cc7-9cd2-659d9e648a9f}
    CapabilityData[0]    0x00
    CapabilityData[1]    0x00
    CapabilityData[2]    0x03
    CapabilityData[3]    0x06
    CapabilityData[4]    0xb2
    CapabilityData[5]    0x00
    CapabilityData[6]    0x02
    CapabilityData[7]    0x00
can't get debug descriptor: Resource temporarily unavailable
Device Status:     0x0001
  Self Powered

```


```
Bytes:
0:
1: D-pad Bitmap
2: Middle Buttons Bitmap
3: ABXY+L1+L2 Bitmap
4: Left Stick X-Axis (right positive) (0x80 center)
5: Left Stick Y-Axis (up negative) (0x80 center)
6: Right Stick X-Axis
7: Right Stick Y-Axis
8: L2 (0-255)
9: R2 (0-255)



X:  03 08 00 10 80 80 80 80 00 00
Up: 03 08 00 00 80 80 80 80 00 00
Y:  03 08 00 08 80 80 80 80 00 00
Up: 03 08 00 00 80 80 80 80 00 00
A:  03 08 00 40 80 80 80 80 00 00
B:  03 08 00 20 80 80 80 80 00 00
D-Left: 0000   03 06 00 00 80 80 80 80 00 00
D-Right:  0000  03 02 00 00 80 80 80 80 00 00
D-Up: 0000   03 00 00 00 80 80 80 80 00 00
D-Down: 0000   03 04 00 00 80 80 80 80 00 00
Three bars: 03 08 20 00 80 80 80 80 00 00
Screenshot: 03 08 01 00 80 80 80 80 00 00
Assistant: 03 08 02 00 80 80 80 80 00 00
Stadia: 03 08 10 00 80 80 80 80 00 00
Three dots: 03 08 40 00 80 80 80 80 00 00
L1: 03 08 00 04 80 80 80 80 00 00
R1: 03 08 00 02 80 80 80 80 00 00

L2: 0000   03 08 00 00 80 80 80 80 07 00
    0000   03 08 04 00 80 80 80 80 fe 00
R2: 0000   03 08 08 00 80 80 80 80 00 ff

Left Stick: Right: 03 08 00 00 b4 78 80 80 00 00


```
