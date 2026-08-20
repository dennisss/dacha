# Optical Motion Capture : Camera Sensors

This page is a general dump of recommendations and options when it comes to sourcing a camera sensor for optical motion capture.

TLDR: We recommend building the custom AR0234 board (this is cheaper than pre-built ones than feeds in the external trigger signal). Best pre-built alternative is based on the OV9281 (`Innomaker CAM-MIPIOV9281 V2`)

## General Guidance

Considerations for camera selection:

- Camera Interface
    - Raspberry Pis only support MIPI interfaces (2 or 4 lane)
    - Many industrial use LVDS or SLVS which will require an FPGA to translate the signals to the Pi.
- Global shutter (instead of rolling shutter) to minimize motion artifacting.
    - Note that there will still be artifacts if exposure time is too high.
    - There are software mechanisms for compensating for rolling shutter effects but are relatively expensive and tricky to integrate into multi-camera systems.
- Must have an external trigger input pin(s).
- Monochrome Preferred (RGB/Color sensors will work)
    - We only care about the brightness of pixels so if the camera separates out RGB colors, then extra work is required to re-merge them.
    - RGB/color sensors have a Bayer filter that splits the light into the three R-G-B wavelength bands. Most IR light will pass straight through the Bayer filter but some small amount (maybe 5% in some sensor datasheets) will be blocked or imperfections in the filter may result in additional distortions so it is optimal to avoid cameras with this filter installed.
        - Note that there are techniques on the internet for removing the Bayer filter but it is requires a lot of tooling and is risky to the camera sensor.
- Raw output
    - Ideally we just get a raw 8-bit list of pixel values out of the camera
    - Other pixels formats like MJPEG/H264 will require more computation (that we don't have) to decode before being processed.
- Higher resolution is better
    - Higher resolution means being able to resolve smaller markers at a given distance / lens field of view.
- Higher frame rate is better
    - If you are tracking objects that move very quickly (e.g. for robotics) you probably need a higher frame rate to be able to provide more updates every second to data consumers (e.g. robotics control algorithms).
- Bigger pixel/sensor size is better.
    - Bigger sensors naturally have higher sensitivity / lower noise so don't require as much light or allow running at higher frame rates for the same light source.
- Don't go larger a "1/2.3inch" optical format sensor
    - This is the biggest size that can be handled by cheap M12 surveillance camera lenses without vignetting.
- BSI vs FSI
    - BSI is recommended but both work fine.
    - BSI ("Back Side Illuminated") means more of the pixel area is actually active so it will have better light sensitivity in the same size sensor and there will be fewer gaps between pixels so its less likely that we will see aliasing artifacts between pixels.

Note that which of these you optimize for depends on the usecase. Typically you will need to decide between resolution and max frame rate. Sensors will also let you use 2x2 binning to get a 1/4 of the resolution at double the framerate.

Connection notes:

- External Trigger
    - Camera boards should ideally take as input the external trigger signal from the carrier board via the GPIO1 pin on the 22-pin connector.
    - Otherwise, you need to wire this pin to the "TRIGGER" connector on the carrier board
- Strobe Output
    - Many cameras allow directly outputting a strobe pulse to control when LEDs are on.
    - This is done separately by the carrier board and you don't need to route this to the camera sensor.
    - Optionally you can disable the carrier board strobe signal in software and route the camera's strobe output to the "STROBE" connector on the carrier board.

## Electrical Design

The most important part of the camera boards is that an extremely low noise analog power supply (typically generated with a high PSRR LDO and avoiding overlaying the analog power plane with other power planes).

Also make sure to follow all the manufacturer's recommendations in terms of capacitor placement (typically you will need a big grid of capacitors directly underneath the sensor).

## Sensor Options 

**AR0234**

- Format: 1/2.6"
- 2/4 lane MPI
- 1920 x 1200 @ 120 FPS
    - 960x600 @ 237 FPS (2x2 binning)
- 3.0um pixels
- Quantum efficiency seems to be better than the AR0235 for 850nm
- Drivers
    - https://forums.raspberrypi.com/viewtopic.php?t=385525
    - https://lore.kernel.org/linux-media/20240614080941.3938212-1-dongcheng.yan@intel.com/
    - https://github.com/Kurokesu/ar0234-v4l2-driver/blob/master/ar0234.c
        - Uses a 24mhz input clock
- FOV:
    - 4.35mm focal length (67 x 45) (76 DFOV)
    - 3.9mm focal length (72 x 49) (82 DFOV)
    - 3.6mm focal length (77 x 53) (87 DFOV)
    - 2.7mm : (94 x 67) (103 DFOV) 
- Best part is the `AR0234CSSM00SUKA0-CP`
    - Want 0 deg CRA
- Prebuilt but expensive board: https://www.kurokesu.com/shop/234x-CSI-M12x
- Image plane: 0.317mm above PCB

**AR0235**

Roughly the same as the AR0234 but harder to get.

- Best part is the `AR0235CSSM00SMKA0-CP`

**OV9281**

- The best camera OV9281 camera board is the `Innomaker CAM-MIPIOV9281 V2`
    - [User Manual](https://www.inno-maker.com/wp-content/uploads/2022/05/CAM-MIPIOV9281-V2-User-Manual-V1.4.pdf)
    - Sensor
        - 1280 x 800 @ 120 FPS
        - 3um x 3um pixels (1/4" sensor)
    - Comes with an M12 lens mount (good size for this usecase)
    - Note that we won't be using the default lens.
- Camera I/O specs:
    - External Trigger
        - Connect `TRIG-` to `GND`
        - Drive a rising edge on `TRIG+` to trigger a frame
            - This has the effect of driving FSIN on the sensor chip high through an optocoupler
    - Strobe Output (TLP281)
        - Connect `Strobe+` to Vcc
        - Pull down `Strobe-` to GND
        - `Strobe-` will be driven high during the strobe.

**Mira220**

- Format: 1/2.7"
- 2.79um pixels
- Best NIR quantum efficiency of any of these sensors (56% at 850nm).
- 2-lane MIPI
- 90fps @ 1600 x 1400 12-bit
- 110fps @ 1280 x 1120 12-bit (cropped)
- https://ams-osram.com/products/sensor-solutions/cmos-image-sensors/ams-mira220
- Best part is the Mira220-2QM1WA
    - Full res with 4.35mm focal length lens
        - DFOV: 68.57
        - HFOV: 54.33
        - VFOV: 48.36

**Python1300 (NOIP1SN1300A-QTI)**

- Format: 1/2"
    - 1280x1024 (4.8um pixels)
    - Best part is "NOIP1FN1300A-QTI" 
- LVDS so will require something like an FPGA
    - Matching LVDS to MIPI transciever FPGA would be a "LIFCL-17-7SG72I"
- Probably the one one that is used in the Optitrack Prime 13
- 20% quantum efficiency at 850nm (NIR version is ~30%)
- Very big sensor and >200 FPS but fairly expensive expensive and requires expensive lenses.

**IMX900**

- IMX900-AMR (https://www.sony-semicon.com/en/products/is/industry/gs/imx900.html)
- Format: 1/3.1" (5.81mm image circle)
- 4-lane CSI
- 2048 x 1636 (3 megapixel) ; 8-bit 125.1 FPS
- (2x2 pinned 8-bit) (0.8 megapixel) : 396.5 FPS
- FOV:
    - 3.9mm focal length (51 x 47) (73 DFOV)
    - 2.7mm focal length (81 x 65) (93 DFOV)
- Image Plane: 0.72mm above PCB.

**Other Omnivision Stuff**

They have a big family of 3.45um pixel size sensors:

- OG02C1B-A88A-001A-Z
    - 1/2.53"
    - 4-lane CSI
    - 1632 x 1264 : 8-bit 300 FPS!!!!
- OG03A1B-C88A-001A
    - full 150fps
    - 1/1.8" (8.9mm image circle)
    - 2064 x 1544
    - Maybe pair with a 6mm lens
        - https://www.aliexpress.us/item/3256808671347641.html
        - Better aperture??
            - https://www.aliexpress.us/item/3256808044248289.html
    - FOV
        - ~6.2mm focal length is good standard FOV (60 x 46) (71 DFOV)
        - 3.9mm focal length is wide angle FOV (85 x 68) (97 DFOV)
- OG05C1B-C88A-001A-Z
    - 1/1.45" (11.12mm image circle)
    - 2464 x 2064
    - Full @ 120 FPS

**Other Sony Stuff**

- Sony Catalog: https://www.sony-semicon.com/en/products/is/industry/global-shutter.html
    - IMX273LLR-C
    - IMX392

**Other Gpixel Stuff**

- GMAX2505
- GMAX4002M


**Other SmartSens Stuff**

- SC130GS-MC1NF00 
- SC132GS
- SC535M
