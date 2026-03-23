38mm 22pin 0.5mm pitch FPC cable for Pi 5 cameras.

The cable is entirely auto

Ordering settings:

- Flex
- 0.1mm PCB thickness
- ENIG
- 0.25mm Polyimide (PI) stiffener on User.Eco1 layer
- Gold Finger Thickness: 0.3mm

Exporting:

```
PCB=pkg/media/camera/boards/camera_ffc/latest/latest.kicad_pcb
OUTPUT_DIR=pkg/media/camera/boards/camera_ffc/latest/plot/

kicad-cli pcb export gerbers \
    --output $OUTPUT_DIR \
    --layers F.Cu,F.Mask,Edge.Cuts,User.Eco1 \
    --exclude-value \
    --exclude-refdes \
    $PCB

```