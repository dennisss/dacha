# Custom TKL Keyboard

## Features

- 87 keys
- Wireless shutoff switch
    - When toggled, the keyboard will only communicate via a USB data connection.

## Dimensions

- Outer (Case): 365mm by 130mm
- PCB: 1mm inset (363mm by 128mm)
- Key Layout: Standard TKL
    - If 1U is 19.05mm
    - Total Key Width: 18.25U = 347.6625mm
    - Total Key Height: 6.5U = 123.82500mm

## Firmware

**Init State**

When the board first powers up, it performs generic setup as follows:
- Set all key row pins as pull-down input.
- Setup I2C running at 400kHz

Then it checks if USB is connected. If it is, then we enter the 'Active' state, else we go to the 'Idle' state.

**Idle State**

In this state, we are waiting for something to happen.

We enter this mode by:
- Setting the voltage of all 16 shift register columns high
- Set interrupt for 5 seconds passed on RTC
- Set GPIOTE PORT interrupt to wake whenever a key row pin goes LOW or the battery counter level changes.
- Set interrupt for USB connected event

Enter a Sleep state (System ON, RAM retention) and go to 'Poll' state when an interrupt is hit.

**Poll State**

- Sent a heartbeat packet over radio
- If a PORT interrupt or USB connected interrupt occured, go to the 'Active' state.

**Active State**

Continously perform the following operations:

- Key scanning thread:
    - Scan every single key for its current state
        - Set all key column register bits to 0
        - Shift out a '1' bit to the first column
        - Check all key row pins.
        - Shift the '1' bit to the next column (shifting out '0' from MCU)
        - Repeat from check step until all keys scanned.
    - If any keys have changed state,
        - Send out a radio/USB packet
- Radio thread
    - If not trying to send a packet, try to receive a radio packet.
- Idle thread
    - If no radio packet/USB packet has been sent in the last 5 seconds, send it.
    - If no key has been pressed DOWN for the last 10 seconds, go to the 'Idle' state

**EEPROM State**

Stores:
- LED presets
- Wireless encryption counters
- Battery discharge counter


## Board

In KiCad the origin is at `(100, 100)` which corresponds to the top-left corner of the keyboard's **case** (PCB starts at `(101, 101)`).

The key matrix is structured as 6 rows of 16 key columns with 88 keys actually connected. Each key signal flows as follows:
- Shift registers output a voltage to each of the 16 columns
- This flows through each switch and it's diode.
- MCU reads key state through individual pins for each of the 6 rows.

- 1N4148
- Kailh Switch Hot Swap Socket
- Durock V2 Stabilizers - Clear Gold Plated PCB Screw-in


### Power Consumption

Individual components:
- NRF52
    - Sleep: 4uA
    - CPU Running: ~5mA
    - Transmitting: ~25mA
- LEDs
    - Sleep: 0
    - Black: 100mA (1mA per LED)
    - White: ~1.2A (12mA per LED)
- OLED
    - Sleep: 10uA
    - On: ~20mA
- LTC4150
    - Operating current: ~90uA
- LM3671
    - 16uA Quiescent current
- 74HC595
    - Idle: 2 * 80uA = 160uA

So total idle power consumption is ~300uA
- ~277 days of battery life if not using radio
- ~100 days of battery life if we send a radio packet for 100ms every 5 seconds.
- ~3 days of battery life if constantly typing with no LEDs.

### Component Selection

**NRF52 DCC to DEC4 Filtering**

- These recommendations are derived from the NRF52 product specification reference circuitry.
- L1: 15nF: High frequency chip inductor (+/-10%) (Footprint: 0603) (Min Footprint: 0402)
- L2: 10uH: Chip inductor, IDC, min 50mA (+/-20%) (Footprint: 0603)
- C5: 1uF: Capacitor, X7R, min 6V rating (+/-10%) (Footprint: 0603)

**Battery**

- Power Input is a 3-pin JST PH
    - Pins are +3.7V, GND, Coulomb Count
- Connect directly to a LTC4150 module:
    - https://www.sparkfun.com/products/12052
- Connect that to a 2000mAh LiPo battery

**USB VBus Reverse Current Protection Diode**

All USB power goes through this diode so we need at least a 1A continous current rating and fairly low voltage drop.

- MBR120 (Used in Adafruit boards)
    - Ideally get an Onsemi part.
    - https://datasheet.lcsc.com/lcsc/1811081334_onsemi-MBR120ESFT1G_C236132.pdf
    - ~0.45V drop at 0.1A
    - ~0.5V drop at 1A
- MBR130
    - Ideally get an Onsemi part.
    - https://www.onsemi.com/pdf/datasheet/mbr130t1-d.pdf
    - ~0.35V drop at 0.1A
    - ~0.47V drop at 1A

**Battery Charge Current Resistor**

4.7K is preferred. This means 200mA of USB power is reserved for charging the battery and 800mAh remains for LEDs


**Mini OLED**

- https://www.adafruit.com/product/661
- SSD1306
- Use I2C
- Sleep mode is 10uA


**SOT-23 P-Channel MOSFET Selection**

- Goal: Optimize for Rds_on @ <1A with -3.3V Vgs
- DM2305 (Used by Adafruit)
    - https://datasheet.lcsc.com/lcsc/1811012320_Diodes-Incorporated-DMG2305UX-7_C150470.pdf
    - 52mOhm Rds @ Vgs = -4.5 with -5A current max
    - 100mOhm Rds @ Vgs = -2.5 with -3.6A current max
    - Expect ~50mOhm Rds
- DMG2301L
    - Much higher Rds
- Si2301
- PJA3415AE
    - https://datasheet.lcsc.com/lcsc/1912111437_PANJIT-International-PJA3415AE-R1-00001_C282373.pdf
    - Expect ~50mOhm Rds


**Power Regulator**

- Goal: Get a voltage of 3.6 - 5V (4.2 on battery) down to 3.3V
- Requirements:
    - 100mA peak. Typical <1mA
- AP2112K-3.3V (Used in Adafruit boards)
    - Simplest / Lowest Part Count: Linear LDO
    - 600mA peak.
    - 55uA Quiescent Current
    - 50uVrms output noise
    - ~75%
    - But in sleep mode, this is the majority of power consumption.
- LM3671
    - DC/DC Step down
    - 600mA peak.
    - 16uA Quiescent Current
    - https://www.ti.com/lit/ds/symlink/lm3671.pdf
    - Total Rds is ~500mOhm
- TLV62569
    - 35uA quescent, 100mO / 60 mO
- TPS62822DLCR

### More Links

- NRF52
    - Product Specification https://infocenter.nordicsemi.com/index.jsp?topic=%2Fps_nrf52840%2Fkeyfeatures_html5.html
- MDBT50-512K (nRF52833)
    - Data Sheet: https://www.raytac.com/download/index.php?index_id=52
- JLCPCB Export Instructions
    - Gerber Files
        - https://support.jlcpcb.com/article/149-how-to-generate-gerber-and-drill-files-in-kicad
    - Assembly Files:
        - https://support.jlcpcb.com/article/153-how-to-generate-bom-and-centroid-files-from-kicad-in-linux
- SK6812-MINI-E
    - https://www.adafruit.com/product/4960
- SK6812B side mount
    - https://www.adafruit.com/product/4691
- Function Switch
    - https://www.digikey.com/en/products/detail/c-k/PCM12SMTR/1640112
- 74HC595 shift register
    - https://cdn-shop.adafruit.com/datasheets/sn74hc595.pdf
- Adafruit example schematic with LiPo support
    - https://cdn-learn.adafruit.com/assets/assets/000/068/545/original/circuitpython_nRF52840_Schematic_REV-D.png?1546364754

## Extra

TODOs:
- Use a smaller footprint for the 74HC595 chips.
    - Also consider finding an I2C version to reduce the pin requirement.
- Must remove most of the B.Paste before sending out for manufacturing.

- Layout Generator:
    - http://www.keyboard-layout-editor.com/#/
    - Could use http://builder.swillkb.com/ to convert to a plate.

- Kicad Plugins
    - https://github.com/adamws/kicad-kbplacer (Better?)
    - https://github.com/yskoht/keyboard-layouter

- Kicad Footprints
    - https://github.com/ai03-2725/MX_Alps_Hybrid
    - https://github.com/perigoso/keyswitch-kicad-library (has 3d + stabilizers)
    - https://github.com/daprice/keyswitches.pretty

- Spacing information:
    - https://www.reddit.com/r/MechanicalKeyboards/wiki/keycap_guides#wiki_key_spacing

- https://github.com/ebastler/kicad-keyboard-parts.pretty    
