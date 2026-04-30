

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

TODO: Make a golden test case:



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
