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

## Design

Below is all the documentation of how thee cameras are designed. These are sorted in the order you would build the camera so while building, skim these and build the things / run the commands listed:

- [Hardware for the cameras](./camera_hardware.md)
- [Setting up the camera OS image](./camera_os.md)
- MCU Firmware
    - TODO: Add complete info on flashing the MCU for the first time.
- Camera Software
    - Installed initially as part of the OS.
    - Mocap Camera Code
        - [//pkg/vision/mocap/camera](/pkg/vision/mocap/camera) This is where the main binary than runs on the camera lives.
    - Connected Components Code
        - [//pkg/vision/src/connected_components](/pkg/vision/src/connected_components)
- [Intrinsics Calibration](./camera_instrinsics.md)

