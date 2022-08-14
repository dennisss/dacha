
lsusb shows:

```
Bus 003 Device 072: ID 0c45:7692 Microdia 
Device Descriptor:
  bLength                18
  bDescriptorType         1
  bcdUSB               1.10
  bDeviceClass            0 
  bDeviceSubClass         0 
  bDeviceProtocol         0 
  bMaxPacketSize0        64
  idVendor           0x0c45 Microdia
  idProduct          0x7692 
  bcdDevice            3.0e
  iManufacturer           1 SONiX
  iProduct                2 USB Keyboard
  iSerial                 0 
  bNumConfigurations      1
  Configuration Descriptor:
    bLength                 9
    bDescriptorType         2
    wTotalLength       0x003b
    bNumInterfaces          2
    bConfigurationValue     1
    iConfiguration          0 
    bmAttributes         0xa0
      (Bus Powered)
      Remote Wakeup
    MaxPower              500mA
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        0
      bAlternateSetting       0
      bNumEndpoints           1
      bInterfaceClass         3 Human Interface Device
      bInterfaceSubClass      1 Boot Interface Subclass
      bInterfaceProtocol      1 Keyboard
      iInterface              0 
        HID Device Descriptor:
          bLength                 9
          bDescriptorType        33
          bcdHID               1.11
          bCountryCode            0 Not supported
          bNumDescriptors         1
          bDescriptorType        34 Report
          wDescriptorLength      79
         Report Descriptors: 
           ** UNAVAILABLE **
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x81  EP 1 IN
        bmAttributes            3
          Transfer Type            Interrupt
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0008  1x 8 bytes
        bInterval               1
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        1
      bAlternateSetting       0
      bNumEndpoints           1
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
          wDescriptorLength     183
         Report Descriptors: 
           ** UNAVAILABLE **
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x82  EP 2 IN
        bmAttributes            3
          Transfer Type            Interrupt
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0010  1x 16 bytes
        bInterval               1
```

Also

```
$ sudo usbhid-dump --model=0c45:7692
003:072:001:DESCRIPTOR         1659901106.350977
 05 0C 09 01 A1 01 85 01 15 00 25 01 75 01 95 20
 09 B5 09 B6 09 B7 09 CD 09 E0 09 E2 09 E3 09 E4
 09 E5 09 E9 09 EA 0A 52 01 0A 53 01 0A 54 01 0A
 55 01 0A 8A 01 0A 21 02 0A 23 02 0A 24 02 0A 25
 02 0A 26 02 0A 27 02 0A 2A 02 0A 92 01 0A 94 01
 0A 83 01 0A 02 02 0A 03 02 0A 07 02 0A 18 02 0A
 1A 02 09 B8 81 02 C0 05 01 09 80 A1 01 85 02 05
 01 19 81 29 83 15 00 25 01 95 03 75 01 81 06 95
 01 75 05 81 01 C0 05 01 09 06 A1 01 85 06 05 07
 75 08 95 01 81 03 15 00 25 01 19 04 29 3B 75 01
 95 38 81 02 19 3C 29 65 75 01 95 2A 81 02 19 85
 29 92 95 0E 81 02 C0

003:072:000:DESCRIPTOR         1659901106.353962
 05 01 09 06 A1 01 05 07 19 E0 29 E7 15 00 25 01
 75 01 95 08 81 02 05 08 75 08 95 01 81 01 19 01
 29 05 75 01 95 05 91 02 75 03 95 01 91 01 05 07
 15 00 26 A4 00 19 00 2A A4 00 75 08 95 06 81 00
 05 0C 09 00 15 80 25 7F 75 08 95 40 B1 02 C0

```