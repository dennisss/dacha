# Camera Interface Utilities

This directory contains utilities (both hardware and software) for dealing with recording 

## Supported Cameras 

This section outlines which cameras are well supported by our software libraries.

### Raspberry Pi CSI Cameras

Basically all Raspberry Pi first/third cameras that connect via the CSI connector are supported. Support for these depends on having the program compiled for `libcamera` support and having the `libcamera` runtime library available in the target machine.

### H.264 USB Cameras

These are USB 1080p cameras which support directly outputting video in H.264 format. Note that most of these cameras use the same interface IC and encode at a constant bit rate (CBR) of around 6Mbps with 1 I-frame per second at 1080p/30fps.

For interfacing with these cameras, we directly use V4L2 (zero library dependencies).

- [Arducam IMX291 Board](https://www.arducam.com/product/arducam-fisheye-low-light-usb-camera-for-computer-2mp-1080p-imx291-wide-angle-mini-h-264-uvc-video-camera-board-with-microphone/) **(recommended)**
    - Sensor: IMX291 (generally the better sensor)
    - Amazon links [1](https://www.amazon.com/Arducam-Camera-Module-IMX291-Microphone/dp/B0861M62KW), [2](https://www.amazon.com/Arducam-Camera-Computer-Microphone-Windows/dp/B07ZRJDTBQ)
    - Pixel Size: 2.9 µm x 2.9 µm
    - Memory chip markings:
        - JD2336 25D20ATIG
        - Probably something like https://www.byte-semi.com/download/SPI_NOR_Flash/BY25D20AS.pdf
        - SOP8 150mil

- [Arducam IMX323](https://www.arducam.com/product/arducam-1080p-low-light-low-distortion-usb-camera-module-with-microphone/)
    - Sensor: IMX323
    - Pixel Size: 2.8μmx 2.8μm

- [ELP AR0330 Board](https://www.amazon.com/dp/B01E8OWZM4) 
    - Sensor: AR0330
    - Sensor Resolution: 2304(H) x 1536(V):
    - Pixel Size: 2.2 um x 2.2um

## Mounting

- Heatsink
    - 20mm x 20mm x 10mm aluminum heatsink
    - Attach with 2mm thick thermal pad

