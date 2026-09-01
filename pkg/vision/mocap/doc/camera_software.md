# Optical Motion Capture : Camera Software

This page aims to describe what all the code that runs on the individual tracking cameras does.

## Linux Kernel

Without the Linux kernel, we depend on the following components:

- Raspberry Pi CFE driver
    - `drivers/media/platform/raspberrypi/rp1_cfe/cfe.c`
    - This drives the MIPI bus and generates all the per-frame userspace events
- Camera Sensor specific driver kernel module
    - Mainly does the I2C and power on/off configuration for the sensor.
- Broadcom PTP
    - `drivers/net/phy/bcm-phy-ptp.c`
    - This does all the time reading/writing from the PTP clock in the Ethernet MAC and also handles scheduling once per second PPS pulses.

## User Space Service

The camera software runs as a continous service where the binary entry point lives in [//pkg/vision/mocap/camera](/pkg/vision/mocap/camera):

- [TimeSyncNode](/pkg/net/ptp/src/node.rs) implements the PTP-like cross-camera syncronization logic.
- [PPSDividerClient](/pkg/vision/mocap/camera/src/pps_divider_client.rs) handles configuration of the MCU.
- [MocapCameraCaptureProcessor](/pkg/vision/mocap/camera/src/capture.rs) handles v4l2 camera polling and scheduling processing
    - Note: We do not use libcamera or the Pi ISP since these are both useless overheads for this project. Instead v4l2 is directly wired up to read raw frames from the sensor.
    - This will use pre-allocated contiguous cacheable memory buffers for storing the generated frames
        - This means they are fast to access and share between the MIPI peripheral and CPU but we need to explicitly clear the CPU cache to avoid reading stale data (since the MIPI interface doesn't invalidate the CPU cache).
    - The software generally follows the following pattern
        - Read V4L2_EVENT_FRAME_SYNC events from the kernel
            - These are received when we get a MIPI "Start of Frame" packet meaning we are about to get the image bytes
        - Based on how much time has elapsed, we will estimate how much of the DMA buffer is full
            - This requires we know the readout speed of the camera (or we measured it from a previous frame)
        - We will clear the caching for that segment of the buffer using `read_partial` (internally calls the custom `dma_buf_sync_partial` ioctl on the DMA buffer)
            - Note that we can't just clear the entire buffer's cache once at the beginning since the CPU's prefetcher is hard to predict and may look ahead to fetch bytes into the cache for regions of the buffer that we haven't deemed to be safe to read yet.  
        - We will incrementally run connected component analysis (CCA) on this batch of lines
        - Continue until we can dequeue the v4l2 buffer (triggered by getting a MIPI "End of Frame" packet)
- [RLEConnectedComponentsProcessor](/pkg/vision/src/connected_components/rle.rs) does the per-line connected component analysis.
    - The output of this is a list of blobs with basic metrics like the centroid, (co)variance, etc. We never store the raw labels map for each pixel.
- [FrameProcessor](/pkg/vision/mocap/camera/core/src/frame_processor.rs) handles filtering of the raw connected components to do a coarse removal of non-circular blobs.
    - It is configured by the `blob_filter` field in the config files (it is a [BlobFilterConfig](/pkg/vision/mocap/proto/processor.proto) data type).
