# Sainsmart 3018 PCB Making

This page is a journal of my process for making engraving PCBs with a SainSmart 3018 CNC.

## Hardware

Specifically we use the following equipment:

- [Genmitsu 3018-PROVer](https://www.sainsmart.com/products/sainsmart-genmitsu-cnc-router-3018-prover-kit)
  - Not using the offline controller (instead connecting via USB).
  - Using the standard 10K RPM spindle.
  - Modify the homing probe to split out to 2 alligator clips
- [3018 MDF Spoilboard](https://www.sainsmart.com/products/genmitsu-cnc-mdf-spoilboard-for-3018-cnc-router-30-x-18-x1-2-cm)
- [T-Track Mini Hold Down Clamps](https://www.sainsmart.com/products/genmitsu-cnc-mdf-spoilboard-for-3018-cnc-router-30-x-18-x1-2-cm)
- [KABA Acrylic Enclosure](https://www.sainsmart.com/products/genmitsu-kaba-desktop-cnc-enclosure)
  - The adhesive on the magnetic strips is very weak so best to bond them with extra Gorilla glue.

TODO: Mention PCB material and bits.

## Software

In this section we will install all the software needed for the process.

The offical documentation for the CNC machine is located below although we will mostly ignore it:

- https://docs.sainsmart.com/3018-prover
- https://docs.sainsmart.com/3018-prover-offline


For controlling the CNC machine we will install grblcontrol (aka Candle). In some directory, run the following:

```bash
# Installing dependencies
sudo apt update
sudo apt install qt5-qmake qtbase5-dev build-essential qtcreator qt5-default libqt5serialport5-dev

git clone https://github.com/Denvi/Candle
cd Candle

mkdir build
cd build
qmake ../src/candle.pro
make -j4
```

Then instead of the `build` directory, you can start it using `./Candle`.

Next we'll install FlatCAM for generating the GCODE. First download the repository into some directory with the following commands:

```
git clone https://bitbucket.org/jpcgt/flatcam
cd flatcam
git checkout origin/Beta
```

As of writing this, the latest commit is `d4f941335ca8a8d5351aab23b396f99da06a9029`. The following edits need to be manually applied:

1. Per the guidance [here](https://gist.github.com/natevw/3e6fc929aff358b38c0a), we need to fix vispy at version 0.6.6 by editing `requirements.txt` as follows:

    ```
    --- a/requirements.txt
    +++ b/requirements.txt
    @@ -14,7 +14,7 @@ setuptools
    dill
    rtree
    pyopengl
    -vispy
    +vispy==0.6.6
    ortools>=7.0
    svg.path>=4.0
    simplejson
    ```

2. `chmod +x ./setup_ubuntu.py`


Then we can continue installing with:

```bash
./setup_ubuntu.sh
pip3 install -r requirements.txt`
```

Finally we can run it using `python3 FlatCAM.py`.

## Exporting from KiCad

Next assuming we have already designed a single or dual layer PCB in KiCad, we will export the needed files to feed into FlatCAM.

When designing the PCB, be aware that if making a single-sided PCB, we will only be milling one side so the through hole components will be mounted on the opposite side of the traces (so use the opposite trace layer on through hole component side).

First open the KiCad PCB view.

Then to export, go to `File > Plot`:

- Select an output directory (e.g. `plot`)
- Select only the `F.Cu` (or `B.Cu`) and `Edge.Cuts` layers
- Hit Plot
- Hit `Generate Drill Files...`
- Enable `PTH and NPTH in single file`
- Hit `Generate Drill File`

At this point you should have three (or four if double sided) files. e.g.:

```
board.drl
board-Edge_Cuts.gbr
board-F_Cu.gbr
```

## Generating GCode

Next we will generate the GCode needed for drilling/milling/engraving the above files.

First open FlatCAM.

- Create a new project if not already in one.
- Open all three files with:
  - `File > Open > Open Gerber...` and `> Open Excellon`
- Verify the orientation
  - Front side traces should be the same orientation as in KiCad
  - Back side traces should be mirrored.
- Select all the objects in the viewer and grab them near the origin (in the +X +Y quadrant).

Then to generate the isolation routing for copper traces:

- Double click on `board-F_Cu.gbr`
- Hit `Isolation Routing`
  - In `Tools Table`, enter `Diameter 0.15, Type: V`  (or 0.21 if using 60 degree)
  - Change Parameters to `3 passes with 10% overlap`
  - Hit `Generate Geometry`
- In the Geometry object:
  - `V-Tip Dia: 0.1`
  - `V-Tip Angle: 30`
  - Travel Z: 2mm
  - Feedrate X/Y: 120 mm/min
  - Feedrate Z: 60 mm/min
  - Spindle: 10000 RPM
  - Rapid Move Feedrate: 1500 mm/min (the default)
  - Hit `Generate CNC Job Object`
- Hit `Save CNC Code` to save the GCode

Then to generate the drill routing:

- Double click on `board.drl`
- Hit `Drilling Tool`
  - Travel XY feed rate: 1500
  - Z feed rate: 40
  - Spindle Speed: 10000 RPM
  - Z Cut Position: -1.7
  - Z Move Position: 2
- Proceed to generate and export the CNC object

Fully generate the edge cuts:

- Double click on `board-Edge_Cuts.gbr`
- Hit `Cutout Tool`
  - 1.2mm bit (3rd from smallest)
  - 1.7mm cut depth, 0.5mm per pass (4 passes)
  - 0.1mm margin
  - No gaps
  - 60mm/min XY cutting speed
  - 40mm/min Z cutting speed
  - Travel Z: 2mm

## Milling the PCB

Next using the Candle program we will mill out the PCB:

- Put on safety glasses
- Tape the PCB to the MDF board and add mounting clamps
- Move the machine to (0, 0, 0) with Candle controls
  - Choose a position over the bottom-left corner of where the PCB will be located.
- Install the engraving bit:
  - Raise the Z until the bit can be inserted.
  - Insert the bit and tighten with a wrench.
- Move up in Z slightly so that the bit is not touching the PCB
- Hit "Zero X/Y"
  - Note: Make sure that the bit doesn't hit any mounting clamps.
- Conduct Z probing
  - Attach one side of the probe to a mounting clamp or the PCB
  - Attach other end to the bit. 
  - Set up a custom probe command in "Settings > Control"
    - `G90; G21; G38.2 Z-50 F100; G92 Z0; G0 Z10; M30`
- Then hit the "Z-probe" button
- Open the F_Cu gcode file
- Under "Heightmap" hit "Create"
- Make it 4x4, hit "Auto" under "Border" and hit "Probe"
- Hit "File > Save as" to save the height map (should be called "height.map")
- Hit "Edit mode" under "Heightmap" to switch back to the gcode job view
- Check "Use heightmap"
- Remove the probe alligator clips
- Close the door to the enclosure
- Hit "Send", then "Ignore"
- It will move above first position.
- Then hit "Pause" to unpause it and start going
- Periodically clean up dust with a handheld vacuum
- After it is done, move back to (0,0) with
  - `G00 X0 Y0`
- Repeat all steps with
  - Drill bit (drill GCode)
  - Routing bit (outline Gcode)
- Be sure to re-open and re-enable the height map when switching files.

## Old

Settings using 3018
- How to do mesh leveling/
- Engraving settings from 'Teaching Tech'
    - For 20 degree v cutter
    - Tool number 10
    - 0.15mm depth
    - 254mm/min feed rate
    - 50mm/min plunge rate
    - 1000 RPM
    - 20% step over


- By default, speed rates would be from 0-1000
    - https://docs.sainsmart.com/article/9m0rbnw6k1-introduction-to-cnc-for-a-total-novice-tuning-gbrl-settings
    - Could set tothe actual RPM range.