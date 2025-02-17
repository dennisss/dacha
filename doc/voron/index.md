TODO: Loctite for bed screws (but need to verify the max service temperature)
- Loctite 248

- TODO: Did I get authentic JST SH male connectors
- TODO: AC power monitoring for the heated bed.


TODO: Will want to have a temperature sensor to measure outside ambient temperature.

Fan 3 pin
- Connector_JST:JST_SH_BM03B-SRSS-TB_1x03-1MP_P1.00mm_Vertical
- LOW
- 5V
- Tach Raw

Would need a higher temp LDO like a `LP2985-33DBVR``

Provok3D bed
- 290W
- 50 ohm DC resistance
- Ambient to 100 degrees C in ~2.5 minutes
- 0.0201388889 W/mm^2

Prusa Mini
- 120W 180x180mm
- 0.0037037037 W/mm^2

Characterizing the heatup
- Energy required to raise heat is 'Q = m * c * (T1 - T0)'
    - 'm' is the mass in grams of the metal
    - 'c' is the specific heat capacity in J/g*C
    - 'Q' is the heat energy in Joules
    - '1 Watt = 1 Joule / second'
- Aluminum has a specific heat capacity of '0.90 J/g*C'
- Cooling can be characterized by 'Newton's law of cooling'
    - 'dT/dt = k * (T(t) - Tambient)'
        - So cooling will be slower if we are closer to the environment temperature
- General model is
    - Inputs:
        - Current bed temperature
        - Current chamber temperature
        - Current ambient temperature
        - Heater power curve
        - 3 state variables (delay between a temperature being reported and )
    - Output:
        - Actual current bed/chamber/ambient temperature
        - Next state variable values
        - Next temperatures in T seconds.


- 43500 joules to get up to 100C
- Estimated = X * 0.9 * 75 * 530 = 35775 - 40545


4 mil is 0.1mm


Remaining things to do:

- BUY MORE FANS
    - At least 1 more 3010 blower and some 4010 ones
- Insulation panels
- Re-print
    - Magfilament parts
        - Also want a pure PCCF version that is annealed.
- Newly print
    - Front of bed bumper
- Figure out the servo and filter cable routing.
- Buy threadlock.
- Design the bedd board and buy parts
    - Will need small standoffs
- Buy bed standoffs and rail screws from mcmaster
- to be safe, get 165mm rails.

need to buy `Connector_JST:JST_SH_SM03B-SRSS-TB_1x03-1MP_P1.00mm_Horizontal`


Filament movement sending
- PAT9125EL-TKIT
    - 850nm light
    - Related https://github.com/markniu/Laser-Filament-Motion-Sensor
    - https://github.com/prusa3d/PRUSA_Laser_filament_sensor
    - https://docs.duet3d.com/Duet3D_hardware/Accessories/Laser_Filament_Monitor
    - https://docs.duet3d.com/Duet3D_hardware/Accessories/Rotating_Magnet_Filament_Monitor
- Can also add a beam reflector to tell if there is a piece of filament in the path
    - Will need to tune this for different filaments and 
- Color sensor?


CNC Bed TODOs
- Smaller spacing around the fan holes
- two fans?
- Some way to mount the wago comment
- Some way to mount the Z-motor nut
- Some way to mount the ground cable
- Space for bumper
- FEM

Will want to pre-insert nuts for the electroncis fanss.

TODO: There might not be enough space for the skirt mesh next to the power plug.

Revo sock

- https://www.printables.com/model/152444-diy-all-around-silicone-sock-for-e3d-revo
    - Smooth-On Mold Max 60
    - https://www.printables.com/model/942746-full-coverage-sock-mold-for-e3d-revo


Want to use 

Chamber thermistors:

- Floor (next to cable chain)
- Right below the bed (ATTiny)
- Toolhead
- Top of the bed rail
- Very top in the hat.
 

Sizing 

- Distance from nozzle to top of floor extrusion is ~159mm
- Linear rail will start 5mm below top of the floor extrusion
    - So nozzle if 159 + 5 = 164mm from bottom of rail
- for a 150mm linear rail, the bottom of the rail carriage is this 164 - 150 + 30.8 (carriage length) = 44.8mm from the hotend
- top of aluminum bed frame is 26.3mm from bottom of rail carriage
- standoff from bed will be 8mm
    - 6mm is what the MK3S uses.
    - Prusa XL is 6mm with insulating foam.


TODO: Must check that the Z nut can do all the way down without hitting the cover or the z motor mount.

- Maybe upgrade to 4010 cowling:
    - https://www.printables.com/model/707229-voron-v02-r1-front-cowling-4010-cooling-fans-beta
- Or add more fans:
    - https://www.printables.com/model/491293-voron-v0-auxiliary-fan-ducts/comments

Solder Stencil Holder:

- https://hackaday.com/2024/10/30/portable-solder-paste-station-prevents-smears-with-suction/
- Best: https://www.printables.com/model/451126-vacuum-pcb-solder-stencil-jig
- https://www.printables.com/model/443582-vacuum-solder-stencil-jig-remix
- JLCPCB stencils aer min 380 x 280
- https://www.youtube.com/watch?v=mEEo1tJj9D8
    - Newer one
- https://github.com/scheffield/stencil-fix
- https://github.com/MariusHeier/magik_solder_paste_stencil_box/



Cutting program:
- Switch to known diameter probe pin
- Touch 4 points to find origin and z offset
- Remove the magnetic connection


Bed/Z Dimensions
- 150mm MGN7H (30.8mm carriage length)
    - So max is 119.2mm of travel
- Standard (CAD) aluminum to bed heater gap is 8mm
- 69mm in X between the rails
- ~17mm from the the back of the top of the linear rail carriage to the first hole 
    - Though roughly closer to 16.85
    - 11.85 from edge of bed

TODO: Need Kirigami 3d printed parts for a 1.9mm sheet.

Still need to buy:
- Pulley
- Bearing
- Motor shaft magnet
- Check fans are PWM'able
    - Fill need back-emf diodes on the fans
- Adafruit LEDs
    - The new ones that I need to self-fabricate
- Mcmaster rods.
- Smaller fuse for the input
- Buy an extra 

Power Supply:
- EPP-200

For the magnetic sensor insert
    - 1.8 by 4.1 is a 'perfect fit'

- LEDs can be either PWM driven or some are the type that use WS2812 protocol

Magnetic filament sensing
- With Chinese 49E (OH49E S49E SS49E, ...)
    - Chip is 2mV / Gauss
        - ~/- 1200 Gauss
    - Midpoint at 3.3V is 1.6V
    - Range of measurement is ~1V to 1.6V
        - Normally drops to 1.3
    - InFiDEL recommends a SS495A

- A 6x3 N52 magnet
    - Need to be at least 3mm away from surface (1,386 Gauss)
    - At 5mm, Gauss is 583
        - 2mm thick is 456
    - https://www.kjmagnetics.com/calculator.asp?srsltid=AfmBOorm981VxM9tt08nL-O3ro66d937v-LuAGSXQJTFu3h4xfmnmkdW


- Should use the improved guidler in https://github.com/VoronDesign/Voron-0/issues/348
    - Maybe drop the screw and just print in pure PCCF

- Need UI option for skipping the pre-heat stage

- What Prusa CORE uses
    - High flow CHT nozzle
    - TMC 2130
        - TMC 2209 should be better.
    - 0.9 degree XY motors (prevents VFA)
    - Genuine Semitec thermistors

- Proper DIN rail system:
    - https://www.printables.com/model/381062-voron-v01-v02-din-rail-board-mounting-system
- TODO: Verify cutoff fuse is high enough temp

- 4010 fan for the kirigami bed

- 4040 fans on the bottom of the printer
- Make sure all the aluminum is grounded
- Need to insulate the front and back sections of the printer
- Ideally split the rear chamber onto a left and right zone so that air can move

- Nema 14 for X/Y
    - LDO-35STH52-1504AH(VRN)
    - 1.5A
    - 0.47 N/cm (0.0046 N-m)
    - 200 steps/rev
    - 24mm 5mm diameter outptu shalf
    - 52mm body
    - May want to test against Moons 0.9 degree motors

- Also check https://ellis3dp.com/Print-Tuning-Guide/articles/useful_macros/hotend_fan_monitoring.html
    - Must avoid sleeve bearing fans

Remaining TODOs
- Print Tulip
    - Need 2 x solid 16T idler
        - https://www.filastruder.com/collections/gates/products/gates-2gt-pulley-custom-no-grub-set-screw?variant=41243545174087
    - Need 2 x 3x18 pins
    - Need 2 x 625 - ZZ 

- Print new Stealthpress
- Print heatset insert guides
- Inventory check
- Test the PZ probe.
- Get a chamber thermistor
- Get another 3010 fan
- Verify we can insert skirt screws when
- Figure out the wiring for the servo and filter
- Raspberry Pi mount.


- Modesty Mush
    - https://www.printables.com/model/407822-voron-v0-modesty-mesh
    - or https://www.printables.com/model/788881-voron-v0-slightly-thicker-modesty-mesh

- Get a Relay
    - Omron G3NA-210B

- Issue with printing the skirts:
    - NEed to make sure there is no Z lift up around one of the thin strips on each end otherwise it will pull off the bed

- More easily removable side panels
    - https://www.printables.com/model/396296-voron-v0-hinged-rear-panel
        - THIS IS FOR V0.1


TODO: Upgrade to something like this
    - https://west3d.com/products/universal-120v-silicone-heater-edge-to-edge-heaters-with-thermal-switch-protection
    - And just cut a hole in the bed to get a better temperature

Voron V0.2r1

- nrf52 pull up is 13kOhm typical (16kOhm peak)

TODO: Buy another 3010 fan for the bed.


- Chamber Thermistor
    - Mainly need one above the bed since that is mainly were it matters.
    - Something like https://www.printables.com/model/499851-voron-v02-chamber-thermistor-mount

TODO: Need to figure out how to route the wiring for the servo and filter

TODO: NEed to verify that fans are PWM drivable.

Mods to use:

- Tulip: https://github.com/Amekyras/tulip

- Kirigami
    - https://github.com/christophmuellerorg/voron_0_kirigami_bed/tree/master
- Kirigami RGB LED?
    - https://github.com/MotorDynamicsLab/LDOVoron0/tree/main/STLs/Kirigami
- Nozzle Wiper 
    - https://mods.vorondesign.com/details/xHsmitgNkpdeQ3tpHImI6A
    - Probably a better brush
        - https://www.printables.com/model/181785-voron-v01-nozzle-brush
        - Ideally upgrade to MG938


- Extra Kirigami Bed Rigidity
    - https://www.printables.com/model/858786-voron-v0-v01-v02-r1-kirigami-bed-brace-fan
    - PCCF
- Pi Camera Mount?
    - https://www.printables.com/model/146877-voron-0-voron-01-raspberry-pi-camera-mount
    - Print later


- Bed alignment clip
- Kirigami Bed Fan (for air circulation)

- 3mm ID Bowden Tube (4mm OD)
    - Since we are going direct drive.

- Removable Back Panel

- Toolhead filament sensor
    -  https://www.printables.com/model/562384-voron-mini-stealthburner-ercf-filament-sensor




Other stuff:
    - https://www.printables.com/model/864913-voron-v02r1-double-shear-ab-motor-mounts
    - https://www.printables.com/model/848709-voron-v0-0-02-v02-improved-guidler
    - https://www.printables.com/model/368008-voron-02-sensor-less-y-axis-bumpers

- https://www.printables.com/model/809509-filtervzero-active-carbon-filter-for-voron-v0
    - 5015 blower fan
    - Prefer 'Delta BFB0524HH'
- https://www.printables.com/model/500513-voron-v0-tiny-recirculating-carbon-filter-mfnano-r


    Aim for any 5015 blower with a rating above 200Pa / 20mmH2O / 1 inH2O.

Quote from nevermore project:

"""
Since the fan (probably) cant be reused for other projects and will function in a high temp environment affecting its lifetime, go for budget. The Sunon Maglev MF50151VX (12V) / MF50152VX (24V) (high speed version, 6000 rpm) is good but unfortunately almost impossible to find as most are fakes. The GDStime 6000rpm Dual Ball bearing is another good option, but quality may vary. The Delta BFB0524HH is a gucci option, as is the special micro versions made for the papst rlf-35 which is an equally awesome and expensive fan. Any fan that works well for stealthburner should be a good option for nevermore micro as well.
"""

https://github.com/nevermore3d/Nevermore_Micro?tab=readme-ov-file#bom-v5


- Filament color sensor https://west3d.com/products/td-1-instant-filament-td-transmissivity-tester-for-hueforge-1-75mm-filament-by-ajax?dt_id=525989



138.92
104.92

130


Note that TMC2240 is a bigger TMC2209 with SPI but doesn't work well with stallguard
- https://klipper.discourse.group/t/sensorless-homing-with-the-tmc-2240-drivers/11438/45
- https://klipper.discourse.group/t/replace-tmc2209-with-tmc2240/8774/3


MCUs
- MDBT50-512K
    - 8.4 x 13.2 x 2.1 size
    - nRF52833
    - 18 GPIO
- MDBT50Q-512K
    - 10.5 x 15.5 x 2.2
    - nRF52833
    - 42 GPIO
- Newer versions of these?
- BMD-380
    - No external caps / inductors
    - Chip antenna
    - 9.5 x 7.5 x 1.5
    - 44 GPIO
    - Up to 85C
- MDBT53V-1M
    - 7.8 x 12.4 x 1.85mm
    - nRF5340
    - Dual core
    - Up to 105C
    - If using DC-DC, then need external components
        - LDO just need 2 caps.


- NINA-B411
    - No antenna
    - 10 x 11.6
    - Less pin density.
- NINA-B301




109.07
124.52667

Bed wires (9 wires)
- AC+
- AC-
- Earth
- Fan Tach
- Fan GND (PWM-ed)
- GND (Temp-, Fan-, )
- LED Data
- 5V (for LED, Fan)
- Temp+

If I had another board
- AC+
- AC-
- Earth
- 5V
- UART
- GND

Then would have onboard connectors for:
- Bed Temp (2 pin)
- Fan (3 pin)
- LED (3 pin)

- Higher temp MCU
    - ATTINY416-MFR
    - Or easier to use is `ATTINY412-SSF`

Connections to RPi

- Ethernet
- 5V power from buck via GPIO
- USB from toolhead
- USB from SKR Pico
- TX/RX from:
    - heated bed
    - left/right LED strip driver (24V driver)
        - Maybe move to the MCU board?
- Camera


Connections to MCU Board

- 24V power
- Motor A
- Motor B
- Motor Z
- Filament Presence Sensor
- SSR Relay for Bed
- Nevermore Fan (24V + tachometer)
- Servo for nozzle brush (5V + PWM)
- Floor thermistor
- Top of beam thermistor
- 2 x 40mm 5V? fans for electronics chamber cooling.
- 2 x 40mm 5V? fans for cooling the bottom chamber with the Z-motor, PSU, SSR, 5V Buck 
- USB to the RPI (if separate)
