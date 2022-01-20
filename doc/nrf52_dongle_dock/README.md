# NRF52840 USB Dongle Dock

Holds the NRF52840 USB Dongle in place with pogo pins to expose the pins for programming via SWD, prototyping, USB connection, etc.

- Use with P75-E2 pogo pins and a 1.6mm thick proto board
- The pogo pins should protrude 1mm out of the bottom of the proto board.

Installation instructions:
1. Place nrf_pogo_holder.stl over a proto build and insert all pogo pins
2. Fix the holder in place using hot glue.
3. Place the nrf_alignment_placeholder.stl on top of the pins and temporarily fix it in place with tape or hot glue.
4. Flip the board over (be careful as the pins may fly out). You should preparably do the assembly on a second flat surface which you can flip along with the board.
5. Solder the pogo pins in place.
6. Remove the alignment placeholder
7. Insert an NRF52840 dongle and fit it in place by sliding on an nrf_brace.stl piece from either side.


12.5 2.54 1.5

Dongle max thickness: ~2.8mm

+ 12.5 pogo body
+ 1.5 pogo tip
+ 2.8 dongle height
- 5 below 3d printed origin
- 1.6 carrier pcb
- 1 stick out of carrier for soldering
+ 1.5 to ensure that the pogo is only depressed by 1mm
