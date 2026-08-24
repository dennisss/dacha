# PPS Divider Project

This project implements a Pulse Per Second (PPS) Divider and Phase-Locked Loop (PLL) for synchronizing camera frames and strobes. It supports two hardware platforms: the **STM32F411 (Blackpill)** and a custom **STM32G031 (Production)** board.

## Build Instructions

### Platform Selection
The project uses a `PLATFORM` variable in the Makefile to select the target device.

*   **STM32F411 (Blackpill)** - *Default*
    ```bash
    make -C pkg/vision/mocap/pps_divider clean
    make -C pkg/vision/mocap/pps_divider PLATFORM=stm32f411
    ```
    *Output:* `build/stm32f411/pps_divider.bin`

*   **STM32G031 (Production)**
    ```bash
    make -C pkg/vision/mocap/pps_divider clean
    make -C pkg/vision/mocap/pps_divider PLATFORM=stm32g031
    ```
    *Output:* `build/stm32g031/pps_divider.bin`

### Flashing
To flash the firmware using `dfu-util` (requires device in DFU mode):
```bash
make flash PLATFORM=<stm32f411|stm32g031>
```

## Hardware Configuration

### STM32G031 (Production)
*   **System Clock**: 64 MHz (HSE Bypass 24 MHz TCXO on **PC14**)
*   **UART**: 115200 8N1 on **PA9** (TX) / **PA10** (RX)
*   **PPS Input**: **PB3** (TIM2_CH2)
*   **Frame Trigger**: **PA15** (TIM2_CH1)
*   **Strobe Trigger**: **PC6** (TIM2_CH3)
*   **ADC Sampling**: 1 kHz (Internal Temp, VCC, POE)

## Directory Structure

*   `src/`
    *   `main.c`, `pll.c`: Common application logic.
    *   `platform/stm32f411/`: Drivers for F411.
    *   `platform/stm32g031/`: Drivers for G031.
*   `inc/`: Common headers.
*   `startup/`: Startup code for each architecture.
*   `ld/`: Linker scripts.

The `Heartbeat` packet (Type 0x04) payload has been updated to include system health and Celsius temperature. It sends 9 bytes of payload:

| Offset | Field | Description |
| :--- | :--- | :--- |
| 0 | `temp_c_half` | Temperature in 0.5°C increments (uint8_t) |
| 1 | `vcc_min` | Minimum VCC observed (u16 ADC) |
| 3 | `vcc_max` | Maximum VCC observed (u16 ADC) |
| 5 | `poe_min` | Minimum POE Voltage observed (u16 ADC) |
| 7 | `poe_max` | Maximum POE Voltage observed (u16 ADC) |

### Voltage Conversion
All ADC values are reported as 12-bit integers (0-4095) normalized to a **3.3V** reference, regardless of the actual VCC supply voltage.

To convert the reported `u16` value to Volts:
```
Voltage (V) = (Value_Reported / 4095.0) * 3.3
```
This applies to `vcc_min/max` and `poe_min/max` fields.

For Temperature (`temp_c_half`):
```
Degrees Celsius = temp_c_half / 2.0
```

## Config Packet (Type 0x01)

| Offset | Field | Description |
| :--- | :--- | :--- |
| 0 | `sequence` | Command sequence number (uint8_t) |
| 1 | `unlock` | Reset PLL lock if non-zero (uint8_t) |
| 2 | `pulse_rate` | Pulses per PPS cycle (uint8_t) |
| 3 | `frame_width` | Frame pulse width in ticks (uint32_t) |
| 7 | `frame_offset` | Shift frame relative to PPS (int32_t) |
| 11 | `strobe_offset`| Shift strobe relative to PPS (int32_t) |
| 15 | `strobe_width` | Strobe pulse width in ticks (uint32_t) |

