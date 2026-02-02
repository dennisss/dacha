

/*

Dual layer workflow:
- Given full workpiece size
- Secure full PCB to the area with weak double sided tape.
- Probe the surface of the PCB
- Drill 4 asymetric alignment holes in the corners
- Verify position of the holes with the camera
- Do the isolation routing for the top side
- Do the solder mask for the top side
- Flip the PCB over
- Check the location of the holes
- Do Z probe
- Transform gcode appropriately
- Do back isolation routing
- Do back solder mask
- Do drilling
- Do edge cuts.


TODO: In the monitor tool, need to move the image preview when the carvera preview is set (but before setting the )

TODO: Disallow doing leveling if we don't have a connection to the wireless probe.


TODO: Need to investigate what the solder mask removal quality is so bad.

TODO: Make this a golden test case:

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/usb_power_switch/usb_power_switch.kicad_pcb \
        --output_path=usb_power_switch.gcode


    cargo run --bin cam --release -- \
        --board_path=pkg/things/fan_controller/boards/board-hl15-latest/board-hl15-latest.kicad_pcb \
        --forced_hole_diameter=0.9 \
        --output_path=fan_controller.gcode


    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/duet3d_alt_magnet/board/board.kicad_pcb \
        --output_path=duet_magnet_stencil.gcode \
        --mode=stencil-front

    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cnc/boards/voron_v0_umbilical/board-latest/board-latest.kicad_pcb \
        single-back \
        --output_path=umbilical.gcode


pkg/cnc/boards/voron_v0_umbilical/board-latest/board-latest.kicad_pcb


    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/voron_v0_bed/board-latest/board-latest.kicad_pcb \
        double-front \
        --output_path=bedboard.gcode

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/voron_v0_bed/board-latest/board-latest.kicad_pcb \
        laser-stencil-front \
        --output_path=bedboard_stencil.svg

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/voron_v0_bed/board-latest/board-latest.kicad_pcb \
        double-back \
        --alignment_data=alignment_data.txtpb \
        --output_path=bedboard_back.gcode

    ======================

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/smart_servo/board/board.kicad_pcb \
        double-front \
        --output_path=servo.gcode

    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cnc/boards/smart_servo/board/board.kicad_pcb \
        double-back \
        --alignment_data=servo_alignment_data.txtpb \
        --output_path=servo_back.gcode

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/smart_servo/board/board.kicad_pcb \
        laser-stencil-back \
        --output_path=servo_stencil_back.svg

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/smart_servo/board/board.kicad_pcb \
        laser-stencil-front \
        --output_path=servo_stencil_front.svg

    ===================

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/attiny_blink/board/board.kicad_pcb \
        single-front \
        --output_path=blink.gcode

    ==================

    cargo run --bin cam --release -- \
        --board_path=pkg/cluster/machines/jbod/boards/backplane-tester/backplane-tester.kicad_pcb \
        double-front \
        --output_path=backplane-tester.gcode

    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cluster/machines/jbod/boards/backplane-tester/backplane-tester.kicad_pcb \
        double-back \
        --alignment_data=backplane_tester_alignment.txtpb \
        --output_path=backplane-tester_back.gcode

    cargo run --bin cam --release -- \
        --board_path=pkg/cluster/machines/jbod/boards/backplane-tester/backplane-tester.kicad_pcb \
        laser-stencil-back \
        --output_path=backplane-tester_back_stencil.svg

    
        


pkg/cnc/boards/voron_v0_main/board/board.kicad_pcb

    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cnc/boards/voron_v0_main/board/board.kicad_pcb \
        double-front \
        --output_path=voron0_main_front.gcode


    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cnc/boards/voron_v0_main/board/board.kicad_pcb \
        double-back \
        --alignment_data=voron0_main_alignment_data.txtpb \
        --output_path=voron0_main_back.gcode


    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cnc/boards/voron_v0_main/board/board.kicad_pcb \
        laser-stencil-back \
        --output_path=voron0_main_back_stencil.svg


pkg/cnc/boards/buck_adapter/board/board.kicad_pcb

    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cnc/boards/buck_adapter/board/board.kicad_pcb \
        single-back \
        --output_path=buck_adapter.gcode

    cargo run --release --bin gcode_tile -- \
        --input=buck_adapter.gcode \
        --output=buck_adapter_tiled.gcode



    cargo run --bin pcb_cam --release -- \
        --config_path=pkg/cnc/cam/config/makera_carvera.txtpb \
        --board_path=pkg/cnc/boards/magnet_sensor/board/board.kicad_pcb \
        single-front \
        --output_path=magnetic_tile.gcode


    ============

    cargo run --bin kicad_export -- --board_path=pkg/cnc/boards/voron_v0_bed/board-latest/board-latest.kicad_pcb --output_dir=/tmp/bedboard


TODO: Verify that we never probe the back side over one of the through holes.

98
108

TODO: For Carvera, explicitly change back to absolute positioning after a toolchange sine it seems to often lose this state.


TODO: For Carvera leveling, if in 'preview' mode, then the view box will move while probing
- Also need a clear sense of the progress o leveling

TODO: Carvera layer previews are broken

TODO: Better carvera vacuum (ideally one with more part visibility)
- For the existing one I ened to do a better job of ensuring that the vacuum tue isn't preventing it from going all the way down.

TODO: Get a replacement Carvera spindle cover

TODO: Need an estimate for how long a whole job will take.
- Challenging part is to estimate the intermediate steps.

TODO: Need an alarm for the UV curing time (ideally make this computer controlled) and when the job is paused, we need user messages.

TODO: Need more tiling simplification:
- Don't need to turn off and on spindle in between the tile runs.

TODO: Solder mask not getting completely removed
- Need to go slower or more overlap?
- It's mostly on the inner most parts which probably have very short cut times
=> Partly fixing with more overlap.

- TODO: chips getting stuck high up on the corn bit

TODO: Still seeing the random flakiness in serial

TODO: Wireless probing auto-suggest # of points based on x/y distance


For stencil need to export an SVG that is inward offset
- https://www.youtube.com/watch?v=mw0mskVCvis


TODO: If we do manual moves while a program is paused, we need to eventually move back to the original position and change back to the original relative/absolute mode to avoid discontinuities in the program.

TODO: When doing a toolchange, reset back to the original feed rate instead of using the last user set one.

TODO: Need validation that nets as well connected if we ignore the fact that most through holes aren't connected.

TODO:  Ideally have a feature in the UI to re-run certain subsets of the gcode (e.g. just redoing a single pad in the solder mask but would need to click on a UI to figure out which one we want to do.)

TODO: Alignment holes don't need to be drilled on the back side.

TODO: Need an option for disabling auto focus

TODO: Need better 

TODO: Always do the isolation pass for the boundary first

TODO: 

*/

fn main() {
    panic!("This doesn't do anything");
}
