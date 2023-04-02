# Video Drivers on RPI

This page documents some of the video devices available on Raspberry Pis.

The inspection commands used can be installed with `sudo apt install v4l-utils`.

## V4L Devices 

A list of what the V4L (`/dev/video*`) devices do:

```
/dev/video10 # H264 -> Raw Image
/dev/video11 # Raw Image -> H264
/dev/video12
/dev/video18
/dev/video31 # Raw -> MJPG
/dev/media3
/dev/video13
/dev/video14
/dev/video15
/dev/video16
/dev/video20
/dev/video21
/dev/video22
/dev/video23
```

# H264 Encoder Info

```
$ v4l2-ctl -d /dev/video11 --all
Driver Info:
	Driver name      : bcm2835-codec
	Card type        : bcm2835-codec-encode
	Bus info         : platform:bcm2835-codec
	Driver version   : 5.15.84
	Capabilities     : 0x84204000
		Video Memory-to-Memory Multiplanar
		Streaming
		Extended Pix Format
		Device Capabilities
	Device Caps      : 0x04204000
		Video Memory-to-Memory Multiplanar
		Streaming
		Extended Pix Format
Media Driver Info:
	Driver name      : bcm2835-codec
	Model            : bcm2835-codec
	Serial           : 0000
	Bus info         : platform:bcm2835-codec
	Media version    : 5.15.84
	Hardware revision: 0x00000001 (1)
	Driver version   : 5.15.84
Interface Info:
	ID               : 0x0300001a
	Type             : V4L Video
Entity Info:
	ID               : 0x0000000f (15)
	Name             : bcm2835-codec-encode-source
	Function         : V4L2 I/O
	Pad 0x01000010   : 0: Source
	  Link 0x02000016: to remote pad 0x1000012 of entity 'bcm2835-codec-encode-proc': Data, Enabled, Immutable
Priority: 2
Format Video Capture Multiplanar:
	Width/Height      : 32/32
	Pixel Format      : 'H264' (H.264)
	Field             : None
	Number of planes  : 1
	Flags             : 
	Colorspace        : Rec. 709
	Transfer Function : Default
	YCbCr/HSV Encoding: Default
	Quantization      : Default
	Plane 0           :
	   Bytes per Line : 0
	   Size Image     : 524288
Format Video Output Multiplanar:
	Width/Height      : 32/32
	Pixel Format      : 'YU12' (Planar YUV 4:2:0)
	Field             : None
	Number of planes  : 1
	Flags             : 
	Colorspace        : Rec. 709
	Transfer Function : Default
	YCbCr/HSV Encoding: Default
	Quantization      : Default
	Plane 0           :
	   Bytes per Line : 64
	   Size Image     : 3072
Selection Video Output: crop, Left 0, Top 0, Width 32, Height 32, Flags: 
Selection Video Output: crop_default, Left 0, Top 0, Width 64, Height 32, Flags: 
Selection Video Output: crop_bounds, Left 0, Top 0, Width 64, Height 32, Flags: 

Codec Controls

             video_bitrate_mode 0x009909ce (menu)   : min=0 max=1 default=0 value=0 flags=update
				0: Variable Bitrate
				1: Constant Bitrate
                  video_bitrate 0x009909cf (int)    : min=25000 max=25000000 step=25000 default=10000000 value=10000000
           sequence_header_mode 0x009909d8 (menu)   : min=0 max=1 default=1 value=1
				0: Separate Buffer
				1: Joined With 1st Frame
         repeat_sequence_header 0x009909e2 (bool)   : default=0 value=0
                force_key_frame 0x009909e5 (button) : flags=write-only, execute-on-write
          h264_minimum_qp_value 0x00990a61 (int)    : min=0 max=51 step=1 default=20 value=20
          h264_maximum_qp_value 0x00990a62 (int)    : min=0 max=51 step=1 default=51 value=51
            h264_i_frame_period 0x00990a66 (int)    : min=0 max=2147483647 step=1 default=60 value=60
                     h264_level 0x00990a67 (menu)   : min=0 max=15 default=11 value=11
				0: 1
				1: 1b
				2: 1.1
				3: 1.2
				4: 1.3
				5: 2
				6: 2.1
				7: 2.2
				8: 3
				9: 3.1
				10: 3.2
				11: 4
				12: 4.1
				13: 4.2
				14: 5
				15: 5.1
                   h264_profile 0x00990a6b (menu)   : min=0 max=4 default=4 value=4
				0: Baseline
				1: Constrained Baseline
				2: Main
				4: High
```

## Supported Image Formats

```
$ v4l2-ctl --list-formats
ioctl: VIDIOC_ENUM_FMT
	Type: Video Capture

	[0]: 'YUYV' (YUYV 4:2:2)
	[1]: 'UYVY' (UYVY 4:2:2)
	[2]: 'YVYU' (YVYU 4:2:2)
	[3]: 'VYUY' (VYUY 4:2:2)
	[4]: 'RGBP' (16-bit RGB 5-6-5)
	[5]: 'RGBR' (16-bit RGB 5-6-5 BE)
	[6]: 'RGBO' (16-bit A/XRGB 1-5-5-5)
	[7]: 'RGBQ' (16-bit A/XRGB 1-5-5-5 BE)
	[8]: 'RGB3' (24-bit RGB 8-8-8)
	[9]: 'BGR3' (24-bit BGR 8-8-8)
	[10]: 'RGB4' (32-bit A/XRGB 8-8-8-8)
	[11]: 'BA81' (8-bit Bayer BGBG/GRGR)
	[12]: 'GBRG' (8-bit Bayer GBGB/RGRG)
	[13]: 'GRBG' (8-bit Bayer GRGR/BGBG)
	[14]: 'RGGB' (8-bit Bayer RGRG/GBGB)
	[15]: 'pBAA' (10-bit Bayer BGBG/GRGR Packed)
	[16]: 'BG10' (10-bit Bayer BGBG/GRGR)
	[17]: 'pGAA' (10-bit Bayer GBGB/RGRG Packed)
	[18]: 'GB10' (10-bit Bayer GBGB/RGRG)
	[19]: 'pgAA' (10-bit Bayer GRGR/BGBG Packed)
	[20]: 'BA10' (10-bit Bayer GRGR/BGBG)
	[21]: 'pRAA' (10-bit Bayer RGRG/GBGB Packed)
	[22]: 'RG10' (10-bit Bayer RGRG/GBGB)
	[23]: 'pBCC' (12-bit Bayer BGBG/GRGR Packed)
	[24]: 'BG12' (12-bit Bayer BGBG/GRGR)
	[25]: 'pGCC' (12-bit Bayer GBGB/RGRG Packed)
	[26]: 'GB12' (12-bit Bayer GBGB/RGRG)
	[27]: 'pgCC' (12-bit Bayer GRGR/BGBG Packed)
	[28]: 'BA12' (12-bit Bayer GRGR/BGBG)
	[29]: 'pRCC' (12-bit Bayer RGRG/GBGB Packed)
	[30]: 'RG12' (12-bit Bayer RGRG/GBGB)
	[31]: 'pBEE' (14-bit Bayer BGBG/GRGR Packed)
	[32]: 'BG14' (14-bit Bayer BGBG/GRGR)
	[33]: 'pGEE' (14-bit Bayer GBGB/RGRG Packed)
	[34]: 'GB14' (14-bit Bayer GBGB/RGRG)
	[35]: 'pgEE' (14-bit Bayer GRGR/BGBG Packed)
	[36]: 'GR14' (14-bit Bayer GRGR/BGBG)
	[37]: 'pREE' (14-bit Bayer RGRG/GBGB Packed)
	[38]: 'RG14' (14-bit Bayer RGRG/GBGB)
	[39]: 'GREY' (8-bit Greyscale)
	[40]: 'Y10P' (10-bit Greyscale (MIPI Packed))
	[41]: 'Y10 ' (10-bit Greyscale)
	[42]: 'Y12P' (12-bit Greyscale (MIPI Packed))
	[43]: 'Y12 ' (12-bit Greyscale)
	[44]: 'Y14P' (14-bit Greyscale (MIPI Packed))
	[45]: 'Y14 ' (14-bit Greyscale)
```


## Refernences

Using V4L2 Encoders/Decoders
- https://lalitm.com/hw-encoding-raspi/
- https://www.codeinsideout.com/blog/pi/set-up-camera/#encoders
    - 

Using video in container:
- https://www.losant.com/blog/how-to-access-the-raspberry-pi-camera-in-docker
- Mount /opt/vc
- /dev/vchiq



