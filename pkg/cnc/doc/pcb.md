# PCB Making Guide

This page documents how to manufacture an already designed KiCAD PCB. 

## Mass Production

If you don't want to make the PCBs yourself, you can export the production gerber files and upload to a service like JLCPCB/PCBWay:

```
PCB=path/to/board.kicad_pcb
OUTPUT_DIR=path/to/plot/

kicad-cli pcb export gerbers \
    --output $OUTPUT_DIR \
    --layers F.Cu,In1.Cu,In2.Cu,In3.Cu,In4.Cu,In5.Cu,In6.Cu,B.Cu,F.Paste,B.Paste,F.Silkscreen,B.Silkscreen,F.Mask,B.Mask,Edge.Cuts \
    --exclude-value \
    --exclude-refdes \
    $PCB

kicad-cli pcb export drill \
    --output $OUTPUT_DIR \
    --format excellon \
    --excellon-units mm \
    --drill-origin absolute \
    --excellon-oval-format alternate \
    --excellon-zeros-format decimal \
    $PCB

```

Note that the `OUTPUT_DIR` must end with a `/`.

## CNC Manufacturing

This section contains the process for making a single or double sided PCB using a CNC machine.

As a prerequisite you need to have either single or double side copper clad FR1 / FR4 fiberglass sheet. FR1 is preferred as it is safer / less abrasive on tooling.

### Design Rules

Note that CNC PCB machining has precision limitations compared to a production PCB house. The following settings are recommended when designing a PCB:

**Netclass defaults**

- Clearance: >= 0.3mm
- Trace Width: >= 0.4mm

**Holes/VIas**

- 0.1" Pitch Hole Size: Use 0.9mm holes in 2mm diameter pads.
- All throughhole components holes should only connect to the bottom (reverse) side
    - You need to be able to access both sides of each hole to solder a jumper between the sides.
    - Right-angle connectors usually can be connected on both sides since they are solderable on both sides.

### Software Binary

For generating CNC/laser cutting patterns, we have a 'pcb_cam' program to generate gcode/svg files.

Prebuilt binary:

- [amd64](https://storage.googleapis.com/da-manual-us/cnc-monitor/releases/cnc-monitor-2024070600-amd64.tar) (Linux x64)

Build it yourself:

```
cargo build --bin pcb_cam --release
strip target/release/pcb_cam
```

### Single Sided PCB

**Step 1** : Mount the copper clad board into the CNC bed on top of an MDF wasteboard using double sided tape.

**Step 2** : Prepare GCode. This command uses the machine settings

(available modes are `single-back` and `single-front` depending on which side has all the traces)

```
cargo run --bin pcb_cam --release -- \
    --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
    --board_path=pkg/cnc/boards/voron_v0_umbilical/board-latest/board-latest.kicad_pcb \
    single-back \
    --output_path=umbilical.gcode
```

**Step 3**: Run the gcode on your machine

**Step 4**: The gcode will stop once engraving is done.

At this point,

- Clean up and sand the copper surface
- Apply solder mask
    - Use a 110 size silk screen and ideally one clean squeegee press.
- Dry the solder mask with UV light and a heat gun
    - You want it to be COMPLETELY CURED

**Step 5**: Continue the gcode.

It will complete the removal of the solder mask and cut out the board.

**Step 6**: Clean and dry off the board

**Step 7**: Bathe the board in liquid tin for 10 minutes

This is highly recommended to make it easier to solder to later. Clean off the residual liquid with later afterwards.

### Double Sided PCB

The double sided process is similar to the single sided process but with alignment holes and doing most steps twice:

**Step 1**: Generate front side gcode

```
PCB=pkg/cnc/boards/voron_v0_main/board/board.kicad_pcb
JOB_NAME=voron_main

cargo run --bin pcb_cam --release -- \
    --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
    --board_path="$PCB" \
    double-front \
    --output_path="${JOB_NAME}_front.gcode"
```

This command will also log where it added alignment holes like the following:

```
Making new alignment holes
Alignment hole: Top Left : x: 53.95, -45.95
Alignment hole: Bottom Left : x: 53.95, -130.05
Alignment hole: Top Right : x: 96.05, -45.95
Alignment hole: Bottom Right : x: 92.05, -130.05
```

**Step 2**: Cut out the front side and solder mask

(steps 3-5 of the single sided procedure)

**Step 3**: Use a camera to find the position of all the alignment holes on the front side.

- For this step, you want to use a camera fixed to the toolhead and looking at the board at a FIXED Z position.
- Jog the center of the camera to the center of each alignment hole and record the X/Y position in the board relative coordinate system.
- DO NOT MOVE THE Z or re-home until you have generated the back gcode

**Step 4**: Flip over the board

(flip along X)

**Step 5**: Find the camera position of all the alignment holes on the back side

- Basically repeat step 3 for the back side 
- Make sure you match up the alignment holes with the old measurements (since everything is not flipped along X)

Make a config file called `alignment_data.txtpb` with the measurements. For each mapping, `point` is the hole location in the CAD (what was dumped by the 'Making new alignment holes' command):

```
mappings {
    point: [130, -102]
    front_measurement: [-111.45, -146.65]
    back_measurement: [-125.95, -148.55]
}
mappings {
    point: [100, -100]
    front_measurement: [-141.45, -144.65]
    back_measurement: [-95.85, -146.65]
}
mappings {
    point: [106, -122]
    front_measurement: [-135.45, -166.65]
    back_measurement: [-102.05, -168.65]
}
mappings {
    point: [132, -128]
    front_measurement: [-109.35, -172.65]
    back_measurement: [-128.05, -174.55]
}
```

**Step 6**: Generate back gcode

```
cargo run --bin pcb_cam --release -- \
    --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
    --board_path=$PCB \
    double-back \
    --alignment_data=alignment_data.txtpb \
    --output_path=${JOB_NAME}_back.gcode
```

**Step 7**: Run the gcode

(from this point, continue the single sided procedure from stp 5).

### Laser Cut Stencil

You can generate an SVG file with vectors to cut out as a laser cutter for one side of the board as follows:

```
cargo run --bin pcb_cam --release -- \
    --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
    --board_path=pkg/cnc/boards/motor_encoder/board/board.kicad_pcb \
    laser-stencil-back \
    --output_path=motor_encoder_stencil.svg
```

Material:
- Millar 4mil (0.1mm) thickness
- Laser settings (for 50W CO2 Laser)
    - 100% speed
    - 30% power
    - 80% current

### Common Mistakes

- Runny Solder Mask
    - If your solder mask comes out of the container in two separated parts (a thick colored part and a liquid), then you need to try to get it to remix or buy new solder mask
    - Try heating the container sideways in an oven at 50 degrees
- Solder mask not drying
    - Use green solder mask
    - Use a heat gun.
    - 10 minutes of drying time can be normal for a full board layer.
- Inconsistent engraving across PCB surface
    - Make sure the surface you taped your board to is flat and your are using strong tape.
    - Before mesh bed leveling the surface, tap the board lightly with a hammer to make sure the tape is fully pressed down.
    - If on back side, then make sure you cleaned up extra solder mask globs on front side.


