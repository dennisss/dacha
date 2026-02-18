
TODO: Clean up this doc.


Generally, turning on a light bulb involved connecting it's wire to LIVE

Relay:
- 60, 130  => 55.62, 105
    => -4.38, -25


130 -> 90

Requirements:

- Wireless or USB control
- Direct power from AC outlet
    - ~100mA peak for NRF52
    - ~100mA peak per relay
- AC Voltage sensing

- Per outlet:
    - US 3-pin plug
    - Relay
    - ACS723 Current Sensing
    - Button to do manual switching
    - RGB LED

- NRF52 has an 8-channel ADC


Things that need calibration:

- Scaling of the voltage input:
    - Measure wall RMS voltage with a multimeter (verify with two multimeters)
    - Measure the scaled down RMS voltage with the NRF52
        - Compare against measured value with two multimeters
    - Calculate a scaling factor and apply.
    - Compare against the expected scaling value for a 100 Ohm resistor 
    - Self Test: average voltage from the differential input is 0V (either when powered on or off)
- ADC Sampling Clock Speed
    - Self Test: Verify best fit with a 60Hz sine wave with >99%
- Phase shift between voltage and current curves
    - Do once per outlet:
        - Plug in a power resistor
        - Measure phase shift between the NRF52 measured AC and current (should be 0 given we are using a 'pure' resistive load) 
        - Verify <10 degrees
- Current zero point
    - Do once per outlet
        - Measure current voltage when 
        - Compare to expected value of ~3.3V/2
- Current/power scaling
    - First calibrate the power resistor
        - Drive 50V DC into it through one or more series bench power supplies
        - Based on the current/power draw, derive a better approximation of the resistance of the resistor
            - Will need to also factor in voltage drop across the wires going to the power supply
            - Compare to the expected resistance
    - Do once per outlet
        - Plug the power resistor into the outlet
        - Measure the current with the NRF52 and scale to the expected value for the resistor.
        - Compare to the expected '200mV/A' (pre-voltage divider) sensitivity expected



Components:

- Relay
    - G5LE
    - https://omronfs.omron.com/en_US/ecb/products/pdf/en-g5le.pdf
    - Need a 100mA transistor
        - AO3422 is 2.1A
    - MBR130HW flyback diode (1A rating) SOD123

- AC current sense
    - ACS723
        - TODO: Check if this can handle 3.3V power (else we need to divide the output)
- AC to DC 5V power module
    - Min 300mA (1.5W)
    - https://www.digikey.com/en/products/detail/recom-power/RAC05-05SK/7603372
- Female power port
    - https://www.digikey.com/en/products/detail/te-connectivity-amp-connectors/3-213598-2/1892746
    - https://www.te.com/usa-en/product-3-213598-2.html
- Male power port

- LEDs: https://www.adafruit.com/product/4684

- AC Voltage Sense
    - ZMPT101B
        - Input: 120V AC RMS (170 peak) limited by 85K ohm resistor
            - So ~2mA input
            - Typically 0.34W
        - Output: Goes through a 100 ohm resistor at 2 mA 
            - So raw output will be 0.2V differential output
            - Link to ground
    - Low pass filter:
        - 10nF capacitor with 10K resistor 
    - Run through a differential low pass filter to restrict it to say 120Hz.
    - Then run through the NRF52 with a 2x gain and measure against the 0.6V internal reference



- Have some load


Current apartment outlets:

- Bathroom
    - FS-325

- Standard U.S. AC power wire coloring:
    - https://www.allaboutcircuits.com/textbook/reference/chpt-2/wiring-color-codes/#:~:text=US%20AC%20power%20circuit%20wiring%20color%20codes&text=The%20protective%20ground%20is%20green,red%2C%20black%2C%20and%20blue.

For a light, we are connecting the hot line to the light.

In case I wanted something battery powered:

- MCP1640


# Old

TPS61322 - 5V variant (also needs input inductor)

MC34063


Requirements:
- Run on AC power
- Dimmer (optional)
- Current monitor (optional)
- WiFi
- Octo-coupled relay

The switching regulator:
  https://www.ti.com/lit/ds/symlink/lm2594.pdf?ts=1601840333282&ref_url=https%253A%252F%252Fwww.google.com%252F

Inductor:
    PE-53810
    https://www.digikey.com/en/products/detail/pulse-electronics-power/PE-53810SNL/1037006

    - L10 in the data sheet

Rectifier?
- https://www.digikey.com/en/products/detail/diodes-incorporated/KBP210G/278625

One-gang electrical box
- 2 x 3 inches


Transformer:
    https://www.digikey.com/en/products/detail/tamura/3FD-424/98323?utm_adgroup=Power%20Transformers&utm_source=google&utm_medium=cpc&utm_campaign=Shopping_Product_Transformers_NEW&utm_term=&utm_content=Power%20Transformers&gclid=Cj0KCQjw5eX7BRDQARIsAMhYLP9wFphIshF_2UXj3aOemutmIqFWYr1uWBTdm1ZHuz3ookg_ZKujcVUaAkKTEALw_wcB

NTC Thermistor
- NTC 5-D7