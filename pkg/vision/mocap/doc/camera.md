# Optical Motion Capture : Camera

This page goes over all the design elements of a single mocap camera:

- Camera Support
    - MIPI 4-lane camera support (up to 6 megapixels @ 120 FPS) 
        - We use an AR0234 which is 2.3 megapixels @ 120 FPS
    - External Trigger Support
        - <50ns error between two cameras on the same network.
- Ethernet PoE+ with PTP support
- High Power 850nm IR LED Ring
    - IR LED intensity is fully programmable
    - Syncronized LED pulsing with camera triggering (no camera strobe pin required)
- RGB Status LEDs
- Accelerometer for vibration monitoring and ground plane calibration
- Support for camera filter switches (`SW` connector)

## Building a Camera

Go through the links in order that are listed in the `Design` section below. Most pages should have a `TLDR` or `Overview` with what's important to know for making a camera.

## Design

Below is all the documentation of how thee cameras are designed. These are sorted in the order you would build the camera so while building, skim these and build the things / run the commands listed:

- [Hardware for the cameras](./camera_hardware.md)
- [Setting up the camera OS image](./camera_os.md)
- [Microcontroller Firmware](./camera_mcu.md)
- [MCU Firmware](./camera_mcu.md)
- [Camera Software](./camera_software.md)
    - (installed initially as part of the OS image).
- [Intrinsics Calibration](./camera_instrinsics.md)

