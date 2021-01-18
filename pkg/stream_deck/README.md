

## Protocol

### Set a button image

Endpoint 0x02 OUT

First 8 bytes if each packet are a header

Bytes:
0: 0x02
1: 0x07
2: 0x09 (Index of the button to change)
3: 0x01 (1 if last packet, 0 otherwise)
4-5: Little endian U16 packet length after the header (probably a u32 hence why the rest of this is 0)
6-7: ? Usually zero

Followed by the JPEG chunk

72 x 72 pixel image per button (180 degree flipped)

### Receive Button Press

Endpoint 0x01 IN

512byte packet received:
- 0x 01 00 0f 00 01 00 00 00 00 00 ...
    - 0f is the number of buttons (15) -> Probably a little endian u16
    - Sometimes the second 01 is a 00 (1 in a different )



## Descriptor

```
Bus 001 Device 002: ID 0fd9:006d Elgato Systems GmbH Stream Deck
Device Descriptor:
  bLength                18
  bDescriptorType         1
  bcdUSB               2.00
  bDeviceClass            0 
  bDeviceSubClass         0 
  bDeviceProtocol         0 
  bMaxPacketSize0        64
  idVendor           0x0fd9 Elgato Systems GmbH
  idProduct          0x006d 
  bcdDevice            2.00
  iManufacturer           1 Elgato
  iProduct                2 Stream Deck
  iSerial                 3 AL43J2C24478
  bNumConfigurations      1
  Configuration Descriptor:
    bLength                 9
    bDescriptorType         2
    wTotalLength       0x0029
    bNumInterfaces          1
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
      bNumEndpoints           2
      bInterfaceClass         3 Human Interface Device
      bInterfaceSubClass      0 
      bInterfaceProtocol      0 
      iInterface              0 
        HID Device Descriptor:
          bLength                 9
          bDescriptorType        33
          bcdHID               1.10
          bCountryCode            0 Not supported
          bNumDescriptors         1
          bDescriptorType        34 Report
          wDescriptorLength     177
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
        wMaxPacketSize     0x0200  1x 512 bytes
        bInterval               1
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x02  EP 2 OUT
        bmAttributes            3
          Transfer Type            Interrupt
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0400  1x 1024 bytes
        bInterval               1
Device Qualifier (for other device speed):
  bLength                10
  bDescriptorType         6
  bcdUSB               2.00
  bDeviceClass            0 
  bDeviceSubClass         0 
  bDeviceProtocol         0 
  bMaxPacketSize0        64
  bNumConfigurations      1


```