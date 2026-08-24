# Optical IR Motion Capture System 

TLDR: Watch this video: https://www.youtube.com/watch?v=kYVqL_DqBis

This project is an optical motion tracking system similar to those provided by companies like OptiTrack / Vicon.

The objective is to track the precise 3d position of objects in a room. This is achieved by attaching distinctive markers to the objects and then triangulating those points in space using multiple cameras looking at the same markers.

Note that the primary goal of this is not to make a "toy" (cost optimized). The goal is to make a "tool" with very good precision so that it can be used outside of purely visual use-cases like animation (e.g. for robotics, research, ground truth for other systems, etc.).

## Safety

Note that the cameras do emit high intensity pulses of IR light. These are generally safe if you are over 3ft away from the cameras and avoid direct eye contact. If you intent on building your own or doing up close work, it is recommended to use good safety glasses like the [NoIR YG3](https://www.noirinsight.com/yg3) (my size is #52).

## Security

Note that the default shipped state of the cameras is insecure so DO NOT directly connect the cameras to the internet.

This is the only easy way to make them plug-n-play without a bunch of cloud based product lockdown. If you are a power user, you can choose to re-flash the software and secure them as you see fit.

## Getting Started

### Prerequisites

Make sure to build or source all the following materials:

- N x [Mocap Cameras](./doc/camera.md)
    - These have onboard compute to do all the pixel-level computer vision and have IR LEDs to see reflective markers.
- 1 x [PoE+ capable network switch](./doc/network_switch.md)
- 1 x Linux/Windows/macOS Host Computer
    - CPU Specs: 4-core 2.2Ghz+, 8Gb RAM (x64 Linux/Windows or Apple Silicon Mac)
    - This is used for aggregating 
    - If you care about latency, prefer a dedicated computer or one with at least 4 cores that can be dedicated to mocap.
    - I'm going to support Linux/Windows/macOS operating systems, but I do all my testing on Linux, so Linux is likely to be the most stable implementation.
- 1 x [Ethernet Adapter](./doc/host_ethernet_adapter.md)
- (N + 1) x Cat 6 or better ethernet cables
    - From the network switch to all cameras + host computer.
    - Measure out your room to figure out how long the cables need to be.
    - I buy the [Cable Matters 10Gbps Snagless Cat 6 Ethernet Cable](https://www.amazon.com/Cable-Matters-Snagless-Ethernet-Black/dp/B007NZGPAY) cables on Amazon and those work well.
- N x 1/4"-20 camera wall mounts
    - These depend a lot on how you plan on mounting the cameras.
    - Cheap [reference mount](https://www.amazon.com/dp/B0776RVX64)
- 1 x [calibration wand](./doc/wand.md)
- M x [Markers](./doc/markers.md) to place on objects or people you want to track.

### Setup

- If you haven't already, assemble the wand
- Mount all the cameras rigidly around your room
- Route 1 ethernet cable per cable back to the network switch
- Route 1 ethernet cable from the network switch to the ethernet jack on host your computer. 
- Plug in the network switch into power
- If the cameras are working, the LEDs on the ethernet jack should be blinking.


### Initial Setup

- Open the host software on your computer.
- The cameras should should show up in the list on the top right side in the "Cameras" page.
    - Verify that the # of cameras is correct.
    - The LEDs on the cameras detected will turn green (and blue if you click on them in the UI).
- By default, all cameras start out as disabled
    - Enable all the cameras by hitting the checkbox in the top-left of the list of cameras.
- Cameras will now start to sync times with each other.
- Increase the FPS to a non-zero value and wave around the wand in front of the cameras.
    - Verify that markers (white dots) appear in the camera previews on the left side of the UI.

### Calibration

TODO: Settings tuning, wanding, and origin setting and verify with wand that tracking is right (see the video)

### API

TODO: Links to API

TODO: Links to software design (so that people understand how to tinker with the configs)

TODO: Troubleshooting