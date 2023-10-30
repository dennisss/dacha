
Goal:
- Create a USB hub which:
    - NRF52 only supports up to 12Mbps so that is probably fast enough for downstream ports.

    - Can independently turn on and off any output 
    - Has a latching current limiter for each output.
- For current limiting:
    - Need 

- Ideally also opto-isolate the data pins

- TS391 voltage compartor


- Main question is how to prevent current flow through the D+/D- pins.

- Architecture:
    - Stage 1: 

    - So I have a USB isolator. In front of that put a current sensor, relay, 

- INA169

- V1 will simply use an existing USB hub and create voltage control on the inputs.


Parts:
- USB2514
    - Reference Design: http://ww1.microchip.com/downloads/en/DeviceDoc/EVB-USB2514B-FS_A1%20%20Evaluation%20Board%20Schematic,%20PDF.pdf
    - SparkFun Reference: https://cdn.sparkfun.com/assets/6/f/1/5/b/Qwiic-USB_Hub.pdf
    - Mainly need a bridge rectifier and choke per port
    - Can get 48-QFN ones
        - https://www.digikey.com/en/products/detail/microchip-technology/USB2514-HZH/3873167
- PRTR5V0U2F
    - TVS Diode Array
- RP2040
- Tactile Switches For enabling each port
- LEDs for monitoring whether the port:
    - Is Powered (On)
    - Is off due to fault (Blinking)
    - Off (Off)
- Quad SR Latch to 
- P-Channel MOSFETs to control USB port power