# AR0234 Camera Board

This is a board for attaching an AR0234 camera sensor to a Raspberry Pi 22-pin connector (2 or 4-lane MIPI).

- PCB Stackup: 6-layer; `JLC06161H-3313`
- Need to use via-in-pad processing (capped/plated over vias)

## BOM

- Sensor
    - https://www.digikey.com/en/products/detail/onsemi/AR0234CSSM00SUKA0-CP/15860697
- Connector
    - https://www.digikey.com/en/products/detail/hirose-electric-co-ltd/FH34SRJ-22S-0-5SH-50/5132526
    - 24mm tape
- Crystal
    - https://www.digikey.com/en/products/detail/ecs-inc/ECS-2520MV-240-DN-TR/16578318
- 1.2V Regulator
    - https://www.digikey.com/en/products/detail/texas-instruments/TLV75512PDYDR/22531581
- 1.8V Regulator
    - https://www.digikey.com/en/products/detail/texas-instruments/TLV74318PDBVR/7593922
- 2.8V Regulator
    - https://www.digikey.com/en/products/detail/texas-instruments/LP5907MFX-2-8-NOPB/3906436

- Level Shifting Transistor
    - https://www.digikey.com/en/products/detail/onsemi/BSS138/244210

- Microcontroller
    - 12mm tape
    - https://www.digikey.com/en/products/detail/microchip-technology/ATTINY402-SSNR/9554946

- 0.1uF 16V 0402 Capacitor
    - https://www.digikey.com/en/products/detail/murata-electronics/GRM155R71C104KA88J/2610892
    - 0.5mm +/- 0.05 height
- 1uF 10V 0402 Capacitor
    - https://www.digikey.com/en/products/detail/murata-electronics/GCM155C71A105KE38D/6012263

- 1.5K 0402 Resistor
    - https://www.digikey.com/en/products/detail/yageo/RC0402FR-071K5L/726519
- 10K 0402 Resistor
    - https://www.digikey.com/en/products/detail/yageo/RC0402JR-0710KL/726418

- 1uF 16V 0603 Capacitor
    - https://www.digikey.com/en/products/detail/yageo/CC0603KRX7R7BB105/2833611
- 10uF 10V 0603 Capacitor
    - https://www.digikey.com/en/products/detail/samsung-electro-mechanics/CL10A106KP8NNNC/3886850



## Notes

- EXTCLK : 24Mhz on the reference board. (VDD_IO / 1.8V)
    - ECS-2520MV-240-DN-TR

- Inputs (3.3V):
    - TRIGGER
    - SCLK
    - SDATA (bidi)
        - Note that the Pi probably has pullups already on one side.

- Power Sequencing
    - (1) Wait for ENABLE input (from Pi) (active high)
    - (2) Turn on 2.8V (analog power)
    - Wait 100us
    - (3) Turn on 1.8V
    - Wait 100us
    - (4) Turn on 1.2V
    - Wait until stable
    - (5) Turn on EXTCLK
    - Wait?
    - (6) Assert RESET_N (pull low) for at least 1ms
    - After ~160000 EXTCLKs for init to finish

- Use an MCU for sequencing
    - ATTINY40-MMHR

- Power Requirements:
    - 2.8V : 60mA typical, 115mA peak
        - This is analog power should needs to be high PSSR (>65dB over wide frequency range)
        - `LP5907MFX-2.8`
            - Enable Pin: Low (default pulldown) turns off the regulator.
            - 1uF input and output caps.
            - https://www.digikey.com/en/products/detail/texas-instruments/LP5907MFX-2-8-NOPB/3906436
            - https://www.ti.com/lit/ds/symlink/lp5907.pdf
            - SOT-23-5 (SC-74A, SOT-753)
    - 1.8V : ~13mA peak
        - `TLV74318PDBVR`
            - https://www.digikey.com/en/products/detail/texas-instruments/TLV74318PDBVR/7593922
            - https://www.ti.com/lit/ds/symlink/tlv743p.pdf?HQS=dis-dk-null-digikeymode-dsf-pf-null-wwe&ts=1771252761776&ref_url=https%253A%252F%252Fwww.ti.com%252Fgeneral%252Fdocs%252Fsuppproductinfo.tsp%253FdistId%253D10%2526gotoUrl%253Dhttps%253A%252F%252Fwww.ti.com%252Flit%252Fgpn%252Ftlv743p
            - SOT-23-5 (SC-74A, SOT-753)
        - `TCR2EF18,LM(CT`
    - 1.2V : 155mA typical, 250mA peak
            - TLV75512PDYDR
                - This has an exposed pad.
            - https://www.ti.com/lit/ds/symlink/tlv755p.pdf?ts=1711517340379&ref_url=https%253A%252F%252Fwww.ti.com.cn%252Fproduct%252Fcn%252FTLV755P
