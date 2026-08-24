# Optical Motion Capture : Camera Microcontroller

The STM32 microcontroller on the camera boards (which we sometimes refer to as the PPS Divider) is primarily responsible for:

- Generating the camera and strobe trigger pulses
- Generating the strobe PWM dimming signal.
- Controlling the WS2812 LEDs.

## TLDR (Flashing)

If you are building a camera, you will need to flash the MCU's firmware.

Either fetch a prebuilt firmware image:

```bash
cargo run --bin source_control -- fetch dist/pkg/vision/mocap/pps_divider.bin
```

Or build it from source:

```bash
./pkg/vision/mocap/pps_divider/build.sh
```

Then plug a camera into the ethernet connected to your computer and wait for it to turn on.

Then run the following to have the Raspberry Pi flash the firmware to the MCU:

```bash
cargo run --bin mocap_deb -- update mcu
```

By default, this command will flash all cameras visible on the network.

Once the MCU is flashed, the camera should be useable in the host software.

## Design

The software for the MCU is located in the [//pkg/vision/mocap/pps_divider](/pkg/vision/mocap/pps_divider/) directory. Fundamentally what it does is:

- Waits for a pair of sequential 1 PPS pulses to come in from the Pi's PTP hardware clock.
    - Each pulse's exact arrival time is captured on a continously running timer (`TIM2`)
- Uses these to calculate the frequency and phase of the PTP clock.
- Schedules pulses using `TIM2` to generate phase aligned trigger pulses for the camera/strobe.
- Over time continues monitoring the 1 PPS pulses and gradually adjusts the internal frequency/phase estimate accordingly.

The MCU is connected to the compute module via UART and that is used for most communication.

The compute module GPIO pins are also connected to the MCU's SWD pins which are bit-banged whenever we want to re-program the MCU.