# Optical Motion Capture : LED Illumination Board

This is the board that shines light on passive reflective markers to make them easier to see.

## Components

### LEDs

We will add LEDs (recommended LED is [SFH 4715AS A01](https://www.digikey.com/en/products/detail/ams-osram-usa-inc/SFH-4715AS-A01/11594703)) around the camera in a ring (of 12 LEDs) to illuminate the markers. The ring pattern is designed so that LEDs are as close as possible to the lens so that ideally most retroreflected light from the LEDs bounces back into the Lens.

Note: The LEDs must be placed far enough away from the lens to not be clipped by the lens but not too far away from the lens that stray rays of light are allowed to hit the lens glass from an extreme angle (is noticeable in the images as a halo effect). This needs to be co-designed with high much vertical distance there is between the top of the LEDs and the top of the lens.

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

### LED Options

The premium options are the "OSLON® Black" style LEDs

- `SFH 4715AS A01`
    - https://www.digikey.com/en/products/detail/ams-osram-usa-inc/SFH-4715AS-A01/11594703
    - https://look.ams-osram.com/m/597061415877617c/original/SFH-4715AS-A01.pdf
    - 70% dimness at 30 degrees off center.

- `SFH 4770S A01`
    - 60 degree half angle / 120 deg overall

- `SFH 4716AS A01` (Very vide angle option)
    - (half angle is 75 deg from center / 150deg overall)
        - Stable power (>90%) through 120deg FOV

### LED Driver

To drive the LEDs, they will be wires in series and driven by a single dimmable constant current source (peak 4A) which is enabled only during the camera's pixel integration period.

**Components**:

WARNING: It's very important to carefully choose good capacitors and inductors for this driver since we are strobing in audible 
frequencies (120 - 240 FPS) so you will hear some buzzing unless you are careful.

- Driver: [TPS922055](https://www.digikey.com/en/products/detail/texas-instruments/tps922054dmtr/22106925)
    - [Datasheet](https://www.ti.com/lit/ds/symlink/tps922052.pdf?ts=1704307356845&ref_url=https%253A%252F%252Fwww.ti.com%252Fsitesearch%252Fen-us%252Fdocs%252Funiversalsearch.tsp%253FlangPref%253Den-US%2526searchTerm%253DTPS922052DMTR%2526nr%253D5)
    - Either TPS922054 or TPS922055 works (TPS922055 is better for EMI)
- Diode `D`
    - [V8PM10S-M3/I](https://www.digikey.com/en/products/detail/vishay-general-semiconductor-diodes-division/V8PM10S-M3-I/7427124)
    - Needs to be 6A 100V rated
    - Premium [option](https://www.digikey.com/en/products/detail/vishay-general-semiconductor-diodes-division/SS10PH10-M3-86A/2152233) used on the TI eval board.
- Inductor `L`
    - The best type in terms of low acoustic noise are the ones labeled as "Molded" "Metal Composite" on DigiKey.
        - Should be shielded.
        - Ideally want at least a 6A+ saturation current. 8A+ is ideal. Ripple may be 1-2 amps in the inductor and we
          don't want to get close to the limit for best acoustic performance.
    - **Recommended** 22uH : [Wurth 74439369220](https://www.digikey.com/en/products/detail/w%C3%BCrth-elektronik/c/25588540)
        - Best efficiency. Modest speed: ~8us LED rise time.
        - Used on the official TI eval board.
        - Use with a >= 400kHz driver frequency.
            - **Recommended**: 800kHz which ends up being a little bit acoustically quieter due to less ripple.
        - Size: 1090
    - 15uH : [Wurth 74439358150](https://www.digikey.com/en/products/detail/w%C3%BCrth-elektronik/74439358150/16370231):
        - (only supported on LED board revision >= 4)
        - Good efficiency. Faster ~5us LED rise time. A bit cheaper.
        - Use with a >= 600kHz driver frequency.
        - Size: 8080
    - Cheaper but more likely to coil whine: [SRP1265A-220M](https://www.digikey.com/en/products/detail/bourns-inc/SRP1265A-220M/4876624)
        - (supported on board revision <= 3)
- R_SENSE:
    - For 4A max, 50mOhm (at least 1/4 or 1/2 watt)
        - We use 2 x 100mOhm resistors in parallel
    - Note that all LED current does through this.
- Vcc Cap
    - 1uF 50V X7R soft terminated
        - Heavily over-rated to use larger dielectric material to dampen vibrations. 
        - https://www.digikey.com/en/products/detail/samsung-electro-mechanics/CL10B105KB9VPJC/20498526
- 1nF caps for filters:
    - https://www.digikey.com/en/products/detail/murata-electronics/GRM1885C1H102JA01D/586943
- Input/Output 2.2uF Capacitor
    - [KRM31KR72A225KH01K](https://www.digikey.com/en/products/detail/murata-electronics/KRM31KR72A225KH01K/4421933)
- 0.01uF input filter
    - https://www.digikey.com/en/products/detail/murata-electronics/GCM1885C2A103JE02J/27381955
- R_Fset
    - **Recommended**: 28kOhm for 800kHz
        - Higher than default frequency to minimize ripple which keeps the setup a bit more acoustically quite.
    - 59kOhm for 400kHz
    - [39kOhm](https://www.digikey.com/en/products/detail/yageo/RC0603FR-0739KL/727195) for ~600KHz

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
    - Overclocking: Switch to a 26.7k resistor for ~0.25 A limit
- dVdt : 100nF cap to GND
- UVLO : 402K and 12.4K rdiv.
- `OVP`: Connect to GND to disable over voltage protection

### RGB Status LEDs

- WS2812B-2020
- Buy from https://www.lcsc.com/product-detail/C965555.html

## Manufacturing Specifications

Basically any stackup with FR4, 4 layer, ENIG plating will work. All the high current traces are really wide and the expectation is that a heatsink will be mounted to deal with the thermals.

Use 'capped' vias when ordering the PCBs (R6+ board revisions)

You can follow the same general pattern as described in the [compute board assembly](../compute/index.md) except the recommended pattern is:

- Solder the side with the LEDs
- Solder the other side (no need to exclude the inductor)
- Install through hole components.

After electronics assembly, use some `DOW 3145` RTV adhesive under the electrolytic capacitor to hold it down (you will need to clamp it for 12 hours to cure). DO NOT USE hot glue since it tends to fall off quickly.

## Heatsink

**DIY Manufacturing**

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

**Pro Process**

Order the `.step` file to be CNC'ed out of 6061 aluminum and ask for it be anodized (matte not glossy).

**Assembly**

For the R1 heatsink:
- The heatsinks are attached to the LED PCB using 0.5mm thick thermal pads and 0.4mm plastic washers/spacers on the screw holes

For the R2 heatsink:
- Directly mount the heatsink to the back of the LED board using thermal paste (`DOW 340` recommended)

## Change Log

**Electronics revisions**

- `R2`
    - First stable; Used in the first youtube video.
    - Uses `R1` heatsink.
    - Uses `R1` case.
- `R3`
    - Minor simplication
    - Uses `R1` heatsink
    - Uses `R1` case.
- `R4`
    - Switching to more compact inductor, quieter components.
    - Switching to just use 2 wider spaced RGB LEDs. 
    - Uses `R1` or `R2` heatsink
    - Uses `R2` case
- `R5`
    - EMI/acoustic noise improvements
    - Uses `R1` or `R2` Heatsink
    - Uses `R2` case
- `R6`
    - Tighter thermal via spacing
    - Now requires getting capped vias during PCB manufacturing
    - Uses `R1` or `R2` Heatsink
    - Uses `R2` case
- `R7`/latest
    - Move label to front of board
    - Unmask the bottom side of IC vias (doesn't really matter since we cap the vias but good for ensuring the standard IC footprint is correct)

Note: All vias under LEDs are intentionally masked so that the heatsink can't electrically arc to the LEDs (tenting is not reliable since solder paste has a way of getting through holes).

**Heatsink revisions**

- `R1`
    - Original design
- `R2`
    - Thinner larger surface area version.
    - Newer LED boards clear up more space below the PCB for the heatsink
    - See the "electronics revisions" for compatibility.

