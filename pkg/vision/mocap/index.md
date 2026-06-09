# Optical IR Motion Capture System 

TLDR: Watch this video: https://www.youtube.com/watch?v=kYVqL_DqBis

This project is an optical motion tracking system similar to those provided by companies like OptiTrack / Vicon.

The objective is to track the precise 3d position of objects in a room. This is achieved by attaching distinctive markers to the objects and then triangulating those points in space using multiple cameras looking at the same markers.

Note that the primary goal of this is not to make a "toy" (cost optimized). The goal is to make a "tool" with very good 

## Hardware

The general architecture of the system is composed of the following main hardware components:

- Markers (on the objects being tracked)
- Motion Tracking Cameras
    - PoE/ethernet connected cameras with onboard compute and additional hardware like LEDs
- PoE Network Switch
- Host Computer
    - Synthesizing the data from all the cameras.

### Markers

The markers should ideally be spheres so that they have an easily identifiable center position. Required characteristics are:

- Large enough to be visible as multiple pixels in the camera
    - I usually use 19mm diameter spheres for general purpose use.
- Retroreflective (most light bounces back to the source)
    - Most objects aren't retroreflective so we will be dimmer in an image so we can identify markers are spots in a well illuminated image that are brighter than everything else.
    - Normally this is achieved with a glass microsphere + metalic base layer paint.
- Size
    - 9mm diameter for finer details like fingers.
    - 14mm diameter is typical for human body points.
    - \>= 19mm diameter for defining bigger rigid bodies (e.g. drones)

Where to buy them?

- Premade ones from [OptiTrack](https://optitrack.com/accessories?cat=markers)
- Other shops available online if you Google search for "mocap markers"

How to make them yourself:

- TODO: Document the 3 ways to do this.


**Active Markers**

If you are tracking something that has a battery, then instead of using 'passive markers', you can attach 'active markers' (wide angle IR LEDs) to your object.

Pros:

- Typically longer range tracking compared to passive markers
- Don't need to attach the IR LED ring to your cameras.

Cons:

- Wastes battery power
- Smaller field of view compared to standard markers


LED Options (only need to drive at around <= 100mA):

- https://www.digikey.com/en/products/detail/ams-osram-usa-inc/SFH-4714B-R33/21700203


#### Light Source

The light that our markers will reflect and our cameras will emit and observe will be 850nm near infrared light.

Ideally you want your scene to be completely dark and only shine a little bit of light at the scene so that there is a high contrast in an image taken of the scene between the retroreflective markers and everything else. The main issue is that indoor lights and the sun through windows will create a lot of noise in this process if we just look at all light like a regular camera does. The solution will be to filter to only observing IR light which is relatively low intensity from the sun.

850nm (most common in indoor motion capture) and 940nm (common in TV remotes) are the most common frequencies available with abundant hardware support. 940nm is technically better since it emitted less by the sun but the downside is that typical silicon image sensors become increasingly less sensitive to light at higher wavelengths so will be much harder to see in general.

### Motion Tracking Cameras

This section explains the design of the mocap cameras. Features:

- Camera Support
    - MIPI 4-lane camera support (up to 6 megapixels @ 120 FPS) 
        - We use an AR0234 which is 2.3 megapixels @ 120 FPS
    - External Trigger Support
        - <50ns error between two cameras on the same network.
- Ethernet PoE+ with PTP support
- High Power IR LED Ring
    - IR LED intensity is fully programmable
    - Syncronized LED pulsing with camera triggering (no camera strobe pin required)
- RGB Status LEDs
- Accelerometer for vibration monitoring and ground plane calibration
- Support for camera filter switches (`SW` connector)

The camera electronics are divided into the following boards found in:

- [./boards/compute](./boards/compute) : Compute Carrier Board
- [./boards/led](./boards/led) : LED Ring Board
- [//pkg/media/camera/boards/camera_ar0234](/pkg/media/camera/boards/camera_ar0234) : AR0234 Camera Board
- [//pkg/media/camera/boards/camera_ffc](/pkg/media/camera/boards/camera_ffc) : 22-pin Flat Flex Cable 

3D Printed and CNC design files are in the [./parts](./parts) folder.

#### Compute

For doing marker recognition in camera images, we will use a Raspberry Pi CM5 (or CM4). Any of them should work but the recommended specs are:

- no-Wifi
- 2GB (as low as it can go so 1GB for the CM4)
- 16GB EMMC (or minimally the lite version)

Specific Models:

- What I currently use: CM5002016
- Cheapest CM4: CM4001000

Note that we will use the Ethernet PTP feature of the CM5 so clone boards will not work well without significant modifications.

- [CM5 Datasheet](https://pip-assets.raspberrypi.com/categories/944-raspberry-pi-compute-module-5/documents/RP-008180-DS-6-cm5-datasheet.pdf?disposition=inline)

- [Offical IO/Carrier Board Kicad Files](https://pip.raspberrypi.com/categories/1098-design-files)

- [Serial Notes](https://www.raspberrypi.com/documentation/computers/configuration.html#cm1-cm3-cm3-and-cm4)
    - Primary one is UART0

**Carrier Board Connector**

(need 2x)

- [DF40C-100DS-0.4V(51)](https://www.digikey.com/en/products/detail/hirose-electric-co-ltd/DF40C-100DS-0-4V-51/1969495)
    - This is the standard part used on the offical CM5 IO board 
    - Cheaper on [LCSC](https://www.lcsc.com/product-detail/C597931.html)
- Overall the CM5 PCB will be 1.5mm above the carrier board PCB
    - Note that there are components on the bottom of the CM5 so there is basically zero clearance for components on the carrier board below the CM5.
    - There is a taller version of the connector but we don't use it.

**Camera Connector**

We aim for the board to be Raspberry Pi camera compatible:

- The standard CM5/RP5 camera connector is 0.5mm pitch with 22 pins
    - Exposes all 4 MIPI lanes
    - (what we use)
- The old CSI connector (used on most camera boards) is 15-pin 1.0mm pitch
    - Exposes only 2 MIPI lanes
    - Buy an adapter cable like this:
        - https://www.pishop.us/product/raspberry-pi-zero-mini-camera-cable-38mm/?srsltid=AfmBOoqGjUmR2GsKmsP6ZGImINLrKVcBaJtso3WF6fwT979VAPVBhP9t
        - https://www.aliexpress.us/item/3256807463693256.html

Camera GPIO Usage:

- GPIO0 : Used as camera 'enable' input to camera
- GPIO1 : Used as external trigger input to camera

The connector we are using on the camera and carrier boards:

- [FH34SRJ-22S-0.5SH(50)](https://www.digikey.com/en/products/detail/hirose-electric-co-ltd/FH34SRJ-22S-0-5SH-50/5132526)
    - (best 0.5mm 22p connector)
    - This is a slightly cheaper option which still has locking and has contacts on both top and bottom
    - Even cheaper on [LCSC](https://www.lcsc.com/product-detail/C2889325.html)

**Heatsink**

Recommend getting the standard Pi CM5 passive heatsink (or [Edatec heatsink](https://www.digikey.com/en/products/detail/edatec/ED-CM4COOLER-B/16683008) for CM4).

There is a fan connector on the board (compatible with 4-pin Raspberry Pi 5 fans) just in case but it is recommended to not use a fan since you don't want any unnecessary vibrations.

#### Power Input

Each camera module will get power over ethernet (PoE+). PoE+ network switches deliver between 42.5V - 57V DC to each device and support up to 25.5W of usable power on each ethernet device (note that not all network switches allow all ports to hit this limit simultaneously).

Expected power consumption:

- Pi CM5 (assuming 100% CPU usage)
    - 10W (Peak)
- Camera sensor
    - 1W (Peak)
- LEDs
    - 5-10W depending on exposure time

TODO: Add a current monitor to the PoE input.

#### PoE Circuitry

Note that we are not isolating the PoE power so the camera will be more efficient but most be sufficiently enclosed so that a user can't touch any power lines.

- Magjack ethernet connector (1Gb/s with PoE+ support)
    - TMJG4926HENL or LPJG0926HENL4R
        - Cheapest to but on LCSC: https://www.lcsc.com/product-detail/C22457393.html
        - `LPJG0926DNL` is without LEDs but is only more economical at high volume. 
    - This is the same part as used on the Raspberry Pis.
- Data Lines TVS Diode
    - 2 x [TPD4EUSB30](https://www.lcsc.com/product-detail/C90627.html)
        - This is the standard part used by the CM5 IO board.
- 2 x rectifiers to extract 48V DC power
    - Ideally 100V max reverse voltage for safety
    - [CD-HD201](https://www.digikey.com/en/products/detail/bourns-inc/cd-hd201/6561443)
    - Slightly cheaper borderline safety option [CD-HD01](https://www.digikey.com/en/products/detail/bourns-inc/CD-HD01/6561441)
- PoE Controller
    - [TI TPS2378](https://www.digikey.com/en/products/detail/texas-instruments/TPS2378DDAR/3431161)
        - [Datasheet](https://www.ti.com/general/docs/suppproductinfo.tsp?distId=10&gotoUrl=https%3A%2F%2Fwww.ti.com%2Flit%2Fgpn%2Ftps2378)
        - Startup inrush current limit is 140mA
        - We have ~75ms to fully charge all capacitors during in-rush (else we need more circuitry to current limit)
        - So ~100-120uF is the max bulk capacitance we can have.
        - Need to wire up `CDB` to followup DC-DC converters to ensure they don't turn on until in-rush phase is done.
        - `CDB` is low when in inrush current mode and high-z the rest of the time.
        - Tie `APD` to low to disable external power adapter input.
    - Components for the controller
        - `C_1 = 0.1 μF, 100 V, 10% ceramic capacitor`
        - `D_1` (TVS diode). `SMAJ58A` recommended
        - `R_DEN = 24.9 kΩ ± 1%`
        - `R_CLS = 63.4 ohm`

#### 5V Buck Converter

This as a converter from the PoE DC voltage to 5V 3A (mainly used by the CM5 CPU):

- [LM65645RZTR](https://www.digikey.com/en/products/detail/texas-instruments/lm65645rztr/26812452)
    - [Datasheet](https://www.ti.com/lit/ds/symlink/lm65645.pdf?ts=1754925441963&ref_url=https%253A%252F%252Fwww.ti.com%252Fpower-management%252Facdc-dcdc-converters%252Fproducts.html)

- Inductor
    - Recommended Part: https://www.digikey.com/en/products/detail/coilcraft/mss1210-153med/21381203
    - Cheaper Part: https://www.digikey.com/en/products/detail/pulse-electronics/PA4320-153NLT/6555163
    - Bigger (needs board change) but cheaper : https://www.digikey.com/en/products/detail/bourns-inc/SRP1770TA-150M/5429636


#### PPS Divider

The carrier board has an STM32 based MCU that acts like a fancy software defined PLL that takes as input the PPS signal from the Raspberry Pi and divides it into 2 pulsed signals (camera trigger and LED strobe trigger) at the camera's FPS. Each of these signals has an independently controllable pulse width an time offset.

**Why this is needed?**

- The Broadcom PHY Driver on the CM4/5 only supports 1 PPS
    - This is fixable but in my experience the driver is also buggy and I have had to fix multiple race conditions (especially since on the CM5 the communication latency is higher due to the RP1) so it is risky to depend on this code for a high frequency signal.
- The Broadcom PHY Hardware on the CM4/5 only supports short pulse widths
    - Need a wider width for some sensors (e.g. Mira220) that use the pulse width as the exposure time
    - Need a wider width for the LED strobe signal
        - Many cameras have a STROBE output for this but this would require running an additional separate wire (we don't have any more pins on the 22 pin connector) to the camera and dealing with varying support for this in camera drivers.
- We often need to offset the STROBE output relative to the TRIGGER output (to avoid capturing images before the LEDs are completely on).
    - Broadcom PHY only exposes 1 output
    - STROBE output signals support this on camera sensors but that requires running an extra pin (see above argument).
- A convenient opportunity to add a TCXO to the board.
    - The crystal I am currently using on the PPS divider is 2.5ppm which is likely over 10x more accurate than the Broadcom PHY's one.
    - In the future, we can use this to 'discipline' the Broadcom PHY's crystal to speed up convergence time of PTP and minimize un-corrected drift over time.

**Parts**
- [STM32G031G8U6](https://www.digikey.com/en/products/detail/stmicroelectronics/STM32G031G8U6/10300275)
    - Cheaper option once in stock: https://www.digikey.com/en/products/detail/stmicroelectronics/STM32G031G4U6/10326694?s=N4IgTCBcDaIMoBUCyBmMBxADCgjOgLCALoC%2BQA
- [ASTX-H11-24.000MHZ-T](https://www.digikey.com/en/products/detail/abracon-llc/ASTX-H11-24-000MHZ-T/3641101?s=N4IgTCBcDaIIIGUAqANAtACQIxbWALAHQAMpAshgFppIgC6AvkA)
    - Cheaper drop-in: https://www.digikey.com/en/products/detail/ecs-inc/ECS-TXO-3225MV-240-TR/10478746
- Isolator: ISO7720FDR


#### Sensor Camera

TLDR: We recommend building the custom AR0234 board (this is cheaper than pre-built ones than feeds in the external trigger signal). Best pre-built alternative is based on the OV9281 (`Innomaker CAM-MIPIOV9281 V2`)

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

Note that which of these you optimize for depends on the usecase. Typically you will need to decide between resolution and max frame rate. Sensors will also let you use 2x2 binning to get a 1/4 of the resolution at double the framerate.

Connection notes:

- External Trigger
    - Camera boards should ideally take as input the external trigger signal from the carrier board via the GPIO1 pin on the 22-pin connector.
    - Otherwise, you need to wire this pin to the "TRIGGER" connector on the carrier board
- Strobe Output
    - Many cameras allow directly outputting a strobe pulse to control when LEDs are on.
    - This is done separately by the carrier board and you don't need to route this to the camera sensor.
    - Optionally you can disable the carrier board strobe signal in software and route the camera's strobe output to the "STROBE" connector on the carrier board.

##### Sensor Options 

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
    - With a 3.6mm lens
        - 77 HDOV
        - 53 VFOV
    - With a 4.35mm lens
        - 67.01 HFOV
        - 44.96 VDOC
- Best part is the `AR0234CSSM00SUKA0-CP`
    - Want 0 deg CRA
- Prebuilt but expensive board: https://www.kurokesu.com/shop/234x-CSI-M12x

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
- 110fps @ 1280 x 1120 12-bit
- https://ams-osram.com/products/sensor-solutions/cmos-image-sensors/ams-mira220
- Best part is the Mira220-2QM1WA
    - Full res with 4.35mm focal length lens
        - DFOV: 68.57
        - HFOV: 54.33
        - VFOV: 48.36

**Other Stuff**

- NOIP1SN1300A-QTI
    - Format: 1/2"
        - 1280x1024 (4.8um pixels)
        - Best part is "NOIP1FN1300A-QTI"
        - Matching FPGA would be a "LIFCL-17-7SG72I"
    - Best parts is 
    - LVDS so will require something like an FPGA
    - Probably the one one that is used in the Optitrack Prime 13
    - 20% quantum efficiency at 850nm (NIR version is ~30%)
    - Very big sensor and >200 FPS but most expensive.
- Omnivision stuff
    - OG02C1B
        - 1/2.53"
        - 4-lane CSI
        - 1632 x 1264 : 8-bit 300 FPS!!!!
    - OG05C1B
- Sony Stuff (https://www.sony-semicon.com/en/products/is/industry/global-shutter.html)
    - IMX273LLR-C
    - IMX392
    - IMX900-AMR (https://www.sony-semicon.com/en/products/is/industry/gs/imx900.html)
        - 1/3.1" (5.81mm image circle)
        - 4-lane CSI
        - 2048 x 1636 ; 8-bit 125.1 FPS
        - (2x2 pinned 8-bit) : 396.5 FPS
    - Pricier? (hard to figure out the raw sensor cost) Hard to directly get the raw sensor.

#### Lens 

Recommended M12 Lens (buy with builtin 850nm IR bandpass filter):

- [4.35mm Focal Length](https://www.aliexpress.us/item/3256804658808702.html) (recommended)
    - Narrower FOV better for long range.
    - 1/2.3" image circle
- [3.6mm Focal Length](https://www.aliexpress.us/item/2251832686546887.html) 
    - Wider than average FOV
    - 1/2.5" image circle

Make sure to apply `Nyogel 767A` damping grease to the lens thread to minimize motion over time.

Note that the LED board tries to pack LEDs as closely as possible to the Lens and the current design only supports a max outer lens diameter of 15mm.

We need to pick a lens to go with the camera sensor to cover a good amount of space in our room without being so wide that objects far appear very small. Also note that FOVs over 70 degrees start having distortion that is more computationally expensive to deal with so is ideally avoided. If using an IR light ring The focal length of the lens must also be compatible with the field of view of the LEDs.

Also note that the Lens MUST NOT have a standard built in IR filter:

- Most lenses regardless of whether or not they say the word "IR" have a IR filter that blocks out light above 650nm.
    - NIR / "No IR" is one form of wording we are looking for but this is super inconsistent between suppliers as most don't bother mentioning any details about the IR filter unless you ask them directly.
    - When in doubt, ask the seller whether or not the lens contains any IR filters.
- The recommended lens has a built in '850nm bandpass filter'
    - This wording is a good sign that it doesn't have the regular 650nm low pass filter since otherwise these statements would be conflicting.
- If you do find a lens with absolutely zero IR filter, you will need to get a separate bandpass 850nm filter glass and glue it to the back of the lens (or in front of the lens if you have a large enough piece of glass and block out any light going into the lens from other angles).

Once you find a lens + 850nm band pass filter combo, make sure to find the exact band width of the filter. Usually either in the filter description page or if you ask the supplier, they will give you a graph of frequency vs 'transmission %'. For the ELP lens, the graph has a peak of >90% transmission in the `830 - 870 nm` range. This will be important for validating compatibility with the LEDs. In general, you want as narrow of a band as possible that also fits the majority of your LED light without filtering.

#### LEDs

We will add LEDs (recommended LED is [SFH 4715AS A01](https://www.digikey.com/en/products/detail/ams-osram-usa-inc/SFH-4715AS-A01/11594703)) around the camera in a ring (of 12 LEDs) to illuminate the markers. The ring pattern is designed so that LEDs are as close as possible to the lens so that ideally most retroreflected lgiht from the LEDs bounces back into the Lens.

General factors to consider for LED solution:

- Need an IR LED that emits around 850nm controid wavelength.
    - Doesn't need to be exact but must be close to the filter's central frequency.
- `Δλ` (aka 'spectral half width' parameter) : A very good value for an LED is 30nm
    - Smaller values are better.
    - A small value means that most of the power is focused on wavelengths close to the centroid wavelength.
    - Too large of a value means that less crisp images and probably more light filtered by the Lens bandpass filter.
    - You can also just good at the wavelength `λ` (on the x axis) vs transmission % (on the y axis) graph in the datasheet. Ideally all the area under the curve is in the region transmitted by your band pass filter.
- Viewing Angle
    - Usually this is measured as the full angle from one side of the lens to the opposite where the start and end points are at 50% of the peak intensity of the LED.
    - Light intensity decreased (roughly linearly) with the angle to the LED center ray.
    - So if the viewing angle matches the DFOV (diagonal FOV) of the lens, then the corners of the image will be 50% dimmer than the center.
    - Pick the LED viewing angle to be '~1.5 x DFOV of the lens'.
        - This will give ~75% dimness at corners compared to the center pixel.
        - So 60deg DFOV lens = ~90deg viewing angle LED
        - For 80deg DFOV lens = ~120deg viewing angle LED
    - Picking too large of a field of view will waste energy without significantly improving image quality.
- Current rating
    - Want high power LEDs to be able to illuminate large rooms.
    - 1A-1.5A continous is generally best to maximize how much of our input power we can use.
    - We mainly just care about peak current rating which is typically a separate number measured under a pulse input and needs to be >=3A
        - The recommended `SFH 4715AS A01` LED is a special version of the `SFH 4715AS` LED designed for high current pulses which is what we want to do.
- Number of LEDs
    - It's easiest to efficiency control a chain of LEDs in series.
    - More LEDs is better since individual LEDs become less efficient at higher currents.
    - The number of LEDs in series is limited by PoE voltage (min 48V) minus LED driver headroom  divided by LED forward current.
        - For recommended LEDs @ 4A, they have 3.4V forward voltage, so (48V - 2V) / 3.4V = max 13 LEDs
        - Since there will also be other losses like in the rectifiers, 12 LEDs is a safer max limit for this situtation.

Note that the LEDs won't be on all the time. Power will only be enabled during the camera exposure period which we will target at 0.25ms to 1ms.

**Non-Square Sensor Trap**

Sensors like the AR0234 are annoying to optimize for since the image width is 1.6x the height so you will end up being tempted to get wide angle LEDs to cover the full diagonal but this will end up being wasteful and make central pixels dimmer. So it's better to just assume that that AR0234 is say a "1600 x 1200" sensor when optimizing for LED FOV and consider the outer '1920 - 1600' columns of the sensor to be "best effort".

##### LED Options

The premium options are the "OSLON® Black" style LEDs

- `SFH 4715AS A01`
    - https://www.digikey.com/en/products/detail/ams-osram-usa-inc/SFH-4715AS-A01/11594703
    - https://look.ams-osram.com/m/597061415877617c/original/SFH-4715AS-A01.pdf
    - 70% dimness at 30 degrees off center.

- `SFH 4716AS A01` (Wide angle option)
    - (half angle is 75 deg from center / 150deg overall)
        - Stable power (>90%) through 120deg FOV



#### LED Driver

To drive the LEDs, they will be wires in series and driven by a single dimmable constant current source (peak 4A) which is enabled only during the camera's pixel integration period.

**Components**:

- [TPS922055](https://www.digikey.com/en/products/detail/texas-instruments/tps922054dmtr/22106925)
    - [Datasheet](https://www.ti.com/lit/ds/symlink/tps922052.pdf?ts=1704307356845&ref_url=https%253A%252F%252Fwww.ti.com%252Fsitesearch%252Fen-us%252Fdocs%252Funiversalsearch.tsp%253FlangPref%253Den-US%2526searchTerm%253DTPS922052DMTR%2526nr%253D5)
- Diode `D`
    - [V8PM10S-M3/I](https://www.digikey.com/en/products/detail/vishay-general-semiconductor-diodes-division/V8PM10S-M3-I/7427124)
    - Needs to be 6A 100V rated
- Inductor `L`
    - Note that this needs to be a low magnetoresistance part to minimize the risk of audible noise since out switching frequency (120 - 240 Hz is in the audible range).
    - Best: [Wurth 7447709220](https://www.digikey.com/en/products/detail/w%C3%BCrth-elektronik/7447709220/1638648)
    - Cheaper but more likely to coil whine: [SRP1265A-220M](https://www.digikey.com/en/products/detail/bourns-inc/SRP1265A-220M/4876624)
    - Premium option used on the TI eval boards: https://www.digikey.com/en/products/detail/w%C3%BCrth-elektronik/74439369220/25588540
- R_SENSE:
    - For 4A max, 50mOhm (at least 1/4 or 1/2 watt)
    - Note that all LED current does through this.
- R_FLT: 100 ohm, 0603
- C_COMP: 1nF, 10V X7R
    - R_COMP
    - R_DAMP

**Dimming**

- Device 'disabled' if EN/PWM is low for >50ms
- Dimming mode selected 300us after EN becomess high for at least 5us

(we will use `Analog Diming`)

- `EN/PWM`
    - Drive from the strobe output of the camera or the PPS divider
- `ADIM/HD`
    - Drive from PWM output from the Pi 
    - First high pulse must be at least 1us in width, then subsequent ones can be 

**Input capactor**:

- `C_in`: 
    - [220uF 100V electrolytic cap](https://www.digikey.com/en/products/detail/rubycon/100ZLJ220M12-5X25/3133967)
        - 12.50mm diameter / 27mm height / 5mm lead spacing
        - [alternative option](https://www.digikey.com/en/products/detail/panasonic-electronic-components/eeu-fr1j221l/3072289) with lower safety margin
    - This capacitor needs to be able to hold/discharge enough energy such that the full LED load can be handled (for the camera's exposure time) without dropping below the target forward voltage of each LED (3.4V x 12 LEDs)
    - Voltage drop for a 0.25ms exposure time is: `(4A * (0.25ms / 1000ms)) / 0.00022 Farads = ~4.5V`
    - So if LED forward voltage is `12 * 3.4 = 40.8V`, then any input voltage down to `40.8 + 4.5 + 2V driver headroo = 47.3V` is fine which should always be the case.

**Capacitor Charge Regulator**

The main issue is that the PoE controller can't handle the initial inrush needed to charge a 220uF capacitor during startup and subsequent recharging will also trigger current spikes so we will explicitly limit current going into the capacitor / LED driver.

[TPS26625DRCR](https://www.digikey.com/en/products/detail/texas-instruments/TPS26625DRCR/9692673)

- [Datasheet](https://www.ti.com/lit/ds/symlink/tps2662.pdf?ts=1711390906073&ref_url=https%253A%252F%252Fwww.ti.com%252Fsitesearch%252Fen-us%252Fdocs%252Funiversalsearch.tsp%253FlangPref%253Den-US%2526searchTerm%253DTPS2662%2526nr%253D211)
- Current limit to 200mA
    - R_ILim = 33.2kOhm
- dVdt : 100nF cap to GND
- UVLO : 402K and 12.4K rdiv.
- `OVP`: Connect to GND to disable over voltage protection


#### IR LED Heatsink

Process for making heatsinks
- Cut out of aluminum
- Sand / debur
- De-grease
- Masking tape the back
- Heat up to 90-100C
- Spray paint
- Let dry for 2 days

Kiri Moto 6061 Aluminum settings:

- 3.175mm tool
    - 14000 RPM
    - Feed: 600
    - Plunge Rate: 50
    - 0.4mm DOC
    - Will probably boost to 700 feed
- 'Ease Down'
    - 30 degree ease
- Operations
    - Rough
    - Helical


#### RGB Status LEDs

- WS2812B-2020
- Buy from https://www.lcsc.com/product-detail/C965555.html
- Pins for SPI0 (will only use MOSI, but others should be left disconnected):


#### Accelerometer

[LIS2DW12TR](https://www.digikey.com/en/products/detail/stmicroelectronics/LIS2DW12TR/7348326680) (LIS2DH12TR should also work)

The accelerometer is used mainly for initializing camera orientation estimates. It is not strictly necessary but simplifies alignment.

[Datasheet](https://www.st.com/resource/en/datasheet/lis2dw12.pdf)

- Up is -Y (same as pixel space)
- Right is -X (same as pixel space)
- Towards Camera is Z

Testing:

```
i2cdetect 1
=> Will use /dev/i2c-1
=> Should see address 0x19 respond
```

#### PCB Specifications

Stackup:
- JLCPCB
    - JLC041611-7628
        - 100 Ohm
            - DP Width: 0.1732 mm
            - DP Gap: 0.15mm
        - 90 Ohm
            - DP Width: 0.2337 mm
            - GP Gap: 0.15 mm
- NextPCB
    - 04161H03-7628 on NextPCB
        - 100 Ohm
            - DP Width: 0.18542 mm
            - DP Spacing: 0.137922 mm
        - 90 Ohm
            - DP Width: 0.24511 mm
            - DP Spacing: 0.138684 mm

PCBs are 48 x 80mm

Recommend getting 2 140x140mm stencils (one for each side of the PCB)

Traces:

- POE Taps
    - 1mm clearance
    - 0.3mm track width

- Both MIPS and ethernet needs to be 100 ohm differential impedance

#### Mechanical Assembly

- CM5 PCB is 1.2mm thick
- CM5 sits 0.5mm above the carrier board
- The CM5 passive heatsink can have a max of 3.5mm of screw inserted
    - Recommended minimum screw insertion is ~1.35mm (M2.5's 0.45 pitch x 3) to get a few threads of grip

Board Spacing (between compute and LED boards):

- Exactly 28mm from top of compute board to bottom of the LED board
- Male 0.1" header has ~2.5mm of insulation.
    - Male header insertion distance is ~6.5mm into the female header
- Female 0.1" header has 8.5mm of insulation
- Bridging using this extension header:
    - https://www.digikey.com/en/products/detail/samtec-inc/SSQ-108-03-G-S/1111553
    - Cheaper tin plated ones: https://www.digikey.com/en/products/detail/samtec-inc/SSQ-108-03-F-S/6692119
    - DO NOT TRY BUYING ON ALIEXPRESS / AMAZON. The cheap ones are typically thinner and don't fit well.

#### Testing

This is testing that can be done on the electronics boards before we setup the final software

Testing the compute board:

- First assemble the board without CM5 / camera / LEDs attached.
- Use a multimeter to verify GND / V_POE / 5V are not connected.
- Provide V_POE and GND via the header
    - Any DC voltage >= 9V is fine here
- Verify 5V output
- Disconnect the test probes
- Plug in regulated PoE input
- Verify 5V output and limited current draw
    - Note: Use a constant current source since PoE will turn itself off without some minimal load applied.
- Flash over USB

TODO: Document LED board testing

TODO: Document flashing and testing the camera boards.

#### 3D Printing Parts

All 3d printed parts should be made of black ASA and scaled to be dimensionally accurate (typically scale X/Y by 100.5%).


#### Fasteners

- Attaching the camera to the 3d printed camera mount
    - 4 x M2 x 4mm
    - 4 x M2 3mm height, 3.5mm diameter heatset inserts
- PCB Sandwich Screws
    - 4 x M2.5 32mm (30mm barely works too)
- Exterior Case
    - 4 x M2 20mm
    - 4 x M2 3mm height, 3.5mm diameter heatset inserts

#### Software

This section explains how to setup the software for an individual camera. This builds on the software located in the following places:

- Raspberry Pi Provisioning Code
    - [//pkg/rpi/doc/compute_module.md](/pkg/rpi/doc/compute_module.md)
    - [//pkg/rpi/index.md](/pkg/rpi/index.md)
    - We will be using a custom Raspberry Pi image that backs in most of the relevant customizations (sensor drivers, custom kernel, etc.)
- Clustering Library
    - [//pkg/cluster/index.md](//pkg/cluster/index.md) : For network coordination of cameras, TLS, software management.
- Mocap Camera Code
    - [//pkg/vision/mocap/camera](/pkg/vision/mocap/camera) This is where the main binary than runs on the camera lives.
- Connected Components Code
    - [//pkg/vision/src/connected_components](/pkg/vision/src/connected_components)

Setting up a camera involves running the following steps (assumes using an eMMC based CM5):

Plug in the camera into the USB port of your computer, then run the following to flash EEPROM customizations and mount the eMMC as a disk on your computer:

```
pkg/rpi/scripts/provision_cm5.sh
```

And flash a Linux image by modifying the below command (most likely the disk argument will be different):

```
cargo build --bin rpi_imager --release

sudo target/release/rpi_imager write \
    --image=$PWD/third_party/pi-gen/deploy/2026-05-20-Daspbian-lite.img.gz \
    --disk=mass-storage-gadget \
    --ssh_public_key=$HOME/.ssh/id_cluster.pub \
	--ip_address=10.1.1.28 \
    --netmask=255.255.0.0 \
    --gateway=10.1.0.1 \
	--hardware_model=cm5-regular \
    --config_txt_patch_file=pkg/vision/mocap/config/camera_config_patch.txt
```

Then plug the camera into your network and run the following to set it up camera as a cluster node:

```
cargo run --bin cluster_cli -- \
  setup_node \
  --zone=home \
  --node_addr=10.1.1.26 \
  --ssh_args="-i ~/.ssh/id_cluster" \
  --node_config_patch='hardware_timestamped_interfaces: "eth0"' \
  --sysctl_patch='net.ipv4.ip_unprivileged_port_start = 200'
```

Then mark the cluster node as a mocap camera:

```
cargo run --bin cluster_cli --  labels set --node_id=m851p50wqzrj4 "mocap_camera=yes"
```

TODO: Integrate this into the previous step.


If this is your first camera, run the following to load the camera software across all mocap cameras in the cluster:

```
cargo run --bin cluster_cli -- start_job pkg/vision/mocap/config/camera.job
```

You can monitor all the created nodes and workers (one worker per camera node to run the software) by running these commands:

```
cargo run --bin cluster_cli -- list nodes
cargo run --bin cluster_cli -- list workers
```

Then flash the PPS divider MCU firmware MCU 

```
# Only need to do once for the first camera.
# TODO: Do this automatically in the next command.
make -C pkg/vision/mocap/pps_divider PLATFORM=stm32g031

cargo run --bin mocap_cli -- flash_mcu \
        --camera_addr=hcgztnjdeqeb8.mocap_camera.worker.home.cluster.internal
```

Turn the camera off (disconnect from power) and then completely back on.

TODO: Remove the above line once the flashing step correctly resets the MCU.

You can use a command like the following to view logs of the software:

```
cargo run --bin cluster_cli -- log --worker_name=mocap_camera.hcgztnjdeqeb8 --latest_attempt
```

You can click on the link in the `list workers` command to get the web UI for the camera.


To collect image frames for camera calibration:

- 20% strobe
- 6000us exposure

Then focus the camera.

Then run:

```
cargo run --bin mocap_cli -- grab_frames \
    --camera_addr=hcgztnjdeqeb8.mocap_camera.worker.home.cluster.internal \
    --output_dir=data/mocap_camera_calib/hcgztnjdeqeb8/
```


