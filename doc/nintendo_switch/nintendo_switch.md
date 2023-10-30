# Homebrewing the switch:

Following https://yuzu-emu.org/help/quickstart/

TODO: Fork all of these repositories.


# Battery Emulation Stuff


Passing through USB-C
- Need to pass through VBUS, GND, CC1 and CC2 pins

- Sparkfun Female breakout:
    - https://www.sparkfun.com/products/15100

- CC1 is A5
- CC2 is B5


- L7805CV (TO-220) to power the micro controller


USB-C power into dock
- 14.5 volt


Battery
- 5-pin
- 3.7V LiPo battery
- Middle pin is 'TH' (thermal)
    - 10K thermistor
    - ~9.1K to ground during room temperature
    - ~3K when hot
- Internal
    - Uses HC77AY (thermal cutoff)


Need to emulate a controller:
- Prior work:
    - https://github.com/mart1nro/joycontrol
    - https://github.com/timmeh87/switchnotes/blob/master/console_pairing_session
    - https://github.com/dekuNukem/Nintendo_Switch_Reverse_Engineering
    - https://github.com/Brikwerk/nxbt
    - https://github.com/shinyquagsire23/HID-Joy-Con-Whispering

Nintendo Switch Main Body Dimensions (without JoyCons)

- 14.3mm height
- 100.8mm width
- 172mm 
- Power Button
    - 4mm diameter
    - Side of circle is 15 mm from edge
    - Roughly centered along height



The total enclosure will be 135mm in width by 200mm in length