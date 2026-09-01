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

NOTE: If you purchased cameras, they will come with some of the below items included. Please see [this page](./doc/retail_models.md) with details on what is included in sold units.

Make sure to build or source all the following materials:

- N x [Mocap Cameras](./doc/camera.md)
    - These have onboard compute to do all the pixel-level computer vision and have IR LEDs to see reflective markers.
- 1 x [PoE+ capable network switch](./doc/network_switch.md)
- 1 x Linux/Windows/macOS Host Computer
    - CPU Specs: 4-core 2.2Ghz+, 8Gb RAM (x64 Linux/Windows or Apple Silicon Mac)
        - Linux (Ubuntu LTS) is currently the most well tested and optimized OS
    - This is used for aggregating the data from many cameras and triangulating to 3D.
    - If you care about latency, prefer a dedicated computer or one with at least 4 cores that can be dedicated to mocap.
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

### Download the host software

Follow the below appropriate instructions for your OS to download the host software ([developer documentation](./doc/host_software.md)):

#### Linux

- Prerequisites
    - TLDR: Skip trying to install these unless you see issues with running the app.
    - NetworkManager
        - Most Linux desktop distros should come with this pre-installed.
        - If not, you may need to manually setup your network interface as described [here](./doc/networking.md).
    - Install `libwebkit2gtk` (required for using the local UI)
        - Most likely you already have this and you can skip installing it if the app loads without explicitly installing it.
        - For Ubuntu/Debian: `sudo apt install libwebkit2gtk-4.1-0`
- Download [mocap-linux-x64.tar.gz](https://dacha.dev/dist/pkg/vision/mocap/app/mocap-linux-x64.tar.gz)
- Extract the file
- Double click on the "mocap" binary to open the app.

#### Windows

- Prerequisites
    - OS Version: Windows 10 or 11 (maybe 8 but I haven't tested)
    - WebView2
        - Should be installed by default. If not, install WebView2 from [here](https://developer.microsoft.com/en-us/Microsoft-edge/webview2).
- TODO

#### MacOS

Note: Only Apple Silicon (M1/M2/... chips) builds are currently distributed.

- Prerequisites
    - None
- TODO


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

### Tuning & Calibration

Now you'll want to tune the settings for your environment. This is explained in [video form here](https://youtu.be/WJ13C_1hDIA?si=4UwEQEMoU51pcZMQ&t=1549)

The general steps are:

- Align the cameras
- Tune strobe power / threshold (and maybe exposure / gain)
    - Try waving around the wand around your space while viewing some of the cameras in "Blobs" mode. You want to see reliable dots appears.
    - WARNING: These settings don't currently save across program restarts so need to be re-entered for now (will be resolved in a future update).
- Ensure that the PTP and PPS columns in the cameras list are ok.
    - First the PTP column
        - There is one **bolded** number which is the PTP leader and this represents the clock sync error to the host.
            - Depending on how good your ethernet port is, this may be higher or lower, but should be <10ms
        - For other cameras, this is the error to the leader
            - Should converge to < 0.1us (< 100ns) for all cameras
    - Then the PPS column
        - For all cameras, this shows the error from PTP time to the camera frame times
        - This column will not converge until the PTP column is stable
        - All rows should similarly reach < 0.1us (< 100ns)
    - Note that on a cold start of the cameras, it currently takes up to 30 seconds for timings to fully stabilize.
        - To be improved with software tuning in the future.
- Crank up the FPS to target final running FPS
- (optional) Wait 10-20 minutes for the system to warm up
- Process the "Start" button in the "Wanding" box in the UI
- Slowly wave around the wand uniformly in your capture volume
    - Cameras see the wand best when the T pattern is directly facing them or at a slight angle.
    - You want to capture enough data so that enough pairs of cameras observe the same wand position to "fully connect all the cameras".
    - Usually you need 100 - 200 deduped frames of results for good quality
- Press the "Process" button in the UI
- Wait for it to complete and then observe that the reprojection error is ok.
    - Good is <0.25
- Hit "Apply" to save the camera parameters
- You can now go the "World" page (linked at the top) to preview the cameras in 3D
- You should be able to wave around the wand and see it appear in 3D
- Put the wand down on the ground in the position you want to be the (0,0,0) origin point of your coordinate system.
    - You should see the 4 wand points still visible in the 3d view.
- Then hit the "Set Origin" button to shift the coordinate system
    - The wand should then show up as centered at the intersection of the red/green/blue axes.
- You are now done with calibration and can immediately start capturing stuff.
- Once you are done, it is recommended to set FPS to zero before powering down the cameras.

### API

The data from the host software can be accessed in realtime from a gRPC client.

See https://github.com/dennisss/mocap-client for example code. The UI itself operates as an RPC client so everything that is do-able in the UI is do-able in the API.

### Advanced Tuning

Currently the default settings in the configs are not yet well tuned. Described below is how to go about tweaking the internals of the algorithms to optimize for your environment. If you find a much better set of parameters, let me know and I'll see if we can move the default values closer to those:

- Find the location of your data directory as described in [this page](./doc/host_software.md).
- Edit the `config-base.pbtxt` file to tweak 
    - The individual nested fields are documented in the `.proto` files ([MocapManagerConfig](/pkg/vision/mocap/proto/manager.proto) is the top level config).
    - Note that edits to this file require a restart of the app to take effect.
- These are likely to be the most interesting fields to tune:
    - `initial_camera_config.blob_filter` : These parameters are applied on each camera to filter out blobs (connected components) in the image that look like sphere projections (circles or ellipsis). 
    - `matching` controls the 2D to 3D triangulation process and will let you tune between maximum precision and maximum robustness
        - e.g. high `min_num_matches` + low `max_reprojection_error` will result in very strict matching and will be unlikely to produce incorrect robusts but will be flakier. Relaxing these allows getting more stability but results may sometimes be worse.
    - `wanding` controls parameters used during the wandin calibration
    - `sphere_projection_filter` is the filter used to reject non-spherical projections
        - Currently this runs on the host software ONLY during wanding.
- You can find more comprehensive design documentation for how the individual algorithms here:
    - [Camera software](./doc/camera_software.md)
    - [Host software](./doc/host_software.md)
