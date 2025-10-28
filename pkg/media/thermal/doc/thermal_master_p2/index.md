
- 256 x 192
    - Top frame: Grayscale 8-bit data
        "- a 256x192 yuyv422 stream that contains a grayscale 8 bits normalized video that is directly exploitable (the 8 bits luminance channel is the temperature data and both chrominance channels are fixed values to give a colorless/grayscale result)"
        "- a 256x384 yuyv422 stream that contains an exact copy of the previous stream in the upper half of the image and a greenish picture in the bottom half. This greenish picture is actually the raw non-normalized sensor values that are actually encoded in gray16le format (just 16 bits per pixel, in one grayscale channel, little-endian)."
    - Bottom: gray16le 't = x / 64 - 273.15'
    - YUYV


- https://www.eevblog.com/forum/thermal-imaging/infiray-and-their-p2-pro-discussion/200/


3474:4281 Raysentek Co.,Ltd Camera



```
$ lsusb -d 3474:4281 -v

Bus 007 Device 019: ID 3474:4281 Raysentek Co.,Ltd Camera
Couldn't open device, some information will be missing
Device Descriptor:
  bLength                18
  bDescriptorType         1
  bcdUSB               2.00
  bDeviceClass          239 Miscellaneous Device
  bDeviceSubClass         2 
  bDeviceProtocol         1 Interface Association
  bMaxPacketSize0        64
  idVendor           0x3474 
  idProduct          0x4281 
  bcdDevice            2.00
  iManufacturer           1 Raysentek Co.,Ltd
  iProduct                2 Camera
  iSerial                 3 202206223
  bNumConfigurations      1
  Configuration Descriptor:
    bLength                 9
    bDescriptorType         2
    wTotalLength       0x00df
    bNumInterfaces          2
    bConfigurationValue     1
    iConfiguration          4 
    bmAttributes         0xc0
      Self Powered
    MaxPower              100mA
    Interface Association:
      bLength                 8
      bDescriptorType        11
      bFirstInterface         0
      bInterfaceCount         2
      bFunctionClass         14 Video
      bFunctionSubClass       3 Video Interface Collection
      bFunctionProtocol       0 
      iFunction               2 
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        0
      bAlternateSetting       0
      bNumEndpoints           1
      bInterfaceClass        14 Video
      bInterfaceSubClass      1 Video Control
      bInterfaceProtocol      0 
      iInterface              2 
      VideoControl Interface Descriptor:
        bLength                13
        bDescriptorType        36
        bDescriptorSubtype      1 (HEADER)
        bcdUVC               1.10
        wTotalLength       0x0035
        dwClockFrequency       15.000000MHz
        bInCollection           1
        baInterfaceNr( 0)       1
      VideoControl Interface Descriptor:
        bLength                18
        bDescriptorType        36
        bDescriptorSubtype      2 (INPUT_TERMINAL)
        bTerminalID             1
        wTerminalType      0x0201 Camera Sensor
        bAssocTerminal          0
        iTerminal               0 
        wObjectiveFocalLengthMin      0
        wObjectiveFocalLengthMax      0
        wOcularFocalLength            0
        bControlSize                  3
        bmControls           0x00000000
      VideoControl Interface Descriptor:
        bLength                13
        bDescriptorType        36
        bDescriptorSubtype      5 (PROCESSING_UNIT)
        bUnitID                 2
        bSourceID               1
        wMaxMultiplier          0
        bControlSize            3
        bmControls     0x00000000
        iProcessing             0 
        bmVideoStandards     0x00
      VideoControl Interface Descriptor:
        bLength                 9
        bDescriptorType        36
        bDescriptorSubtype      3 (OUTPUT_TERMINAL)
        bTerminalID             3
        wTerminalType      0x0101 USB Streaming
        bAssocTerminal          0
        bSourceID               2
        iTerminal               0 
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
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        1
      bAlternateSetting       0
      bNumEndpoints           0
      bInterfaceClass        14 Video
      bInterfaceSubClass      2 Video Streaming
      bInterfaceProtocol      0 
      iInterface              0 
      VideoStreaming Interface Descriptor:
        bLength                            14
        bDescriptorType                    36
        bDescriptorSubtype                  1 (INPUT_HEADER)
        bNumFormats                         1
        wTotalLength                   0x006b
        bEndpointAddress                 0x81  EP 1 IN
        bmInfo                              0
        bTerminalLink                       3
        bStillCaptureMethod                 0
        bTriggerSupport                     0
        bTriggerUsage                       0
        bControlSize                        1
        bmaControls( 0)                     0
      VideoStreaming Interface Descriptor:
        bLength                            27
        bDescriptorType                    36
        bDescriptorSubtype                  4 (FORMAT_UNCOMPRESSED)
        bFormatIndex                        1
        bNumFrameDescriptors                2
        guidFormat                            {32595559-0000-0010-8000-00aa00389b71}
        bBitsPerPixel                      16
        bDefaultFrameIndex                  1
        bAspectRatioX                       0
        bAspectRatioY                       0
        bmInterlaceFlags                 0x00
          Interlaced stream or variable: No
          Fields per frame: 2 fields
          Field 1 first: No
          Field pattern: Field 1 only
        bCopyProtect                        0
      VideoStreaming Interface Descriptor:
        bLength                            30
        bDescriptorType                    36
        bDescriptorSubtype                  5 (FRAME_UNCOMPRESSED)
        bFrameIndex                         1
        bmCapabilities                   0x00
          Still image unsupported
        wWidth                            256
        wHeight                           194
        dwMinBitRate                 11919360
        dwMaxBitRate                 23838720
        dwMaxVideoFrameBufferSize       99328
        dwDefaultFrameInterval         400000
        bFrameIntervalType                  1
        dwFrameInterval( 0)            400000
      VideoStreaming Interface Descriptor:
        bLength                            30
        bDescriptorType                    36
        bDescriptorSubtype                  5 (FRAME_UNCOMPRESSED)
        bFrameIndex                         2
        bmCapabilities                   0x00
          Still image unsupported
        wWidth                            256
        wHeight                           386
        dwMinBitRate                 23715840
        dwMaxBitRate                 47431680
        dwMaxVideoFrameBufferSize      197632
        dwDefaultFrameInterval         400000
        bFrameIntervalType                  1
        dwFrameInterval( 0)            400000
      VideoStreaming Interface Descriptor:
        bLength                             6
        bDescriptorType                    36
        bDescriptorSubtype                 13 (COLORFORMAT)
        bColorPrimaries                     1 (BT.709,sRGB)
        bTransferCharacteristics            1 (BT.709)
        bMatrixCoefficients                 4 (SMPTE 170M (BT.601))
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        1
      bAlternateSetting       1
      bNumEndpoints           1
      bInterfaceClass        14 Video
      bInterfaceSubClass      2 Video Streaming
      bInterfaceProtocol      0 
      iInterface              0 
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x81  EP 1 IN
        bmAttributes            5
          Transfer Type            Isochronous
          Synch Type               Asynchronous
          Usage Type               Data
        wMaxPacketSize     0x0400  1x 1024 bytes
        bInterval               1


$ sudo v4l2-ctl -d /dev/video0 --all
Driver Info:
	Driver name      : uvcvideo
	Card type        : Camera: Camera
	Bus info         : usb-0000:0c:00.3-2.2.1.1.2
	Driver version   : 6.5.13
	Capabilities     : 0x84a00001
		Video Capture
		Metadata Capture
		Streaming
		Extended Pix Format
		Device Capabilities
	Device Caps      : 0x04200001
		Video Capture
		Streaming
		Extended Pix Format
Media Driver Info:
	Driver name      : uvcvideo
	Model            : Camera: Camera
	Serial           : 202206223
	Bus info         : usb-0000:0c:00.3-2.2.1.1.2
	Media version    : 6.5.13
	Hardware revision: 0x00000200 (512)
	Driver version   : 6.5.13
Interface Info:
	ID               : 0x03000002
	Type             : V4L Video
Entity Info:
	ID               : 0x00000001 (1)
	Name             : Camera: Camera
	Function         : V4L2 I/O
	Flags            : default
	Pad 0x01000007   : 0: Sink
	  Link 0x0200000d: from remote pad 0x100000a of entity 'Processing 2' (Video Pixel Formatter): Data, Enabled, Immutable
Priority: 2
Video input : 0 (Camera 1: ok)
Format Video Capture:
	Width/Height      : 256/386
	Pixel Format      : 'YUYV' (YUYV 4:2:2)
	Field             : None
	Bytes per Line    : 512
	Size Image        : 197632
	Colorspace        : sRGB
	Transfer Function : Rec. 709
	YCbCr/HSV Encoding: ITU-R 601
	Quantization      : Default (maps to Limited Range)
	Flags             : 
Crop Capability Video Capture:
	Bounds      : Left 0, Top 0, Width 256, Height 386
	Default     : Left 0, Top 0, Width 256, Height 386
	Pixel Aspect: 1/1
Selection Video Capture: crop_default, Left 0, Top 0, Width 256, Height 386, Flags: 
Selection Video Capture: crop_bounds, Left 0, Top 0, Width 256, Height 386, Flags: 
Streaming Parameters Video Capture:
	Capabilities     : timeperframe
	Frames per second: 25.000 (25/1)
	Read buffers     : 0


$ sudo v4l2-ctl -d /dev/video1 --all
Driver Info:
	Driver name      : uvcvideo
	Card type        : Camera: Camera
	Bus info         : usb-0000:0c:00.3-2.2.1.1.2
	Driver version   : 6.5.13
	Capabilities     : 0x84a00001
		Video Capture
		Metadata Capture
		Streaming
		Extended Pix Format
		Device Capabilities
	Device Caps      : 0x04a00000
		Metadata Capture
		Streaming
		Extended Pix Format
Media Driver Info:
	Driver name      : uvcvideo
	Model            : Camera: Camera
	Serial           : 202206223
	Bus info         : usb-0000:0c:00.3-2.2.1.1.2
	Media version    : 6.5.13
	Hardware revision: 0x00000200 (512)
	Driver version   : 6.5.13
Interface Info:
	ID               : 0x03000005
	Type             : V4L Video
Entity Info:
	ID               : 0x00000004 (4)
	Name             : Camera: Camera
	Function         : V4L2 I/O
Priority: 2
Format Metadata Capture:
	Sample Format   : 'UVCH' (UVC Payload Header Metadata)
	Buffer Size     : 10240

```