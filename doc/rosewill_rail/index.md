# Rosewill / TC-RAIL Short Rack Adapter

If you're like me, you have a short [Rosewill 4U Server (RSV-R4000U)](https://www.amazon.com/gp/product/B09HLCNKM3) and want to rack mount it in a short rack (mine is 21.5in end to end). The best metal rail set I've found is the [TC-RAIL-20](https://www.newegg.com/p/1B4-0048-00003) but it only supports racks with minimum depth 22.3in. This adapter piece shortens the rails to fit on down to a 20in depth rack.

Instructions:

- Print 2 x `rail-adapter.stl` part (print 4 if using a <21.5in rack).
    - Recommended Print Settings
        - Filament: PCCF
        - Nozzle Size: 0.6mm
        - "External perimeters first": Enabled
        - Extrusion Multiplier: 0.94
        - Extrusion Width: 0.6 on the external perimeters
        - Nozzle Size: 0.6mm
        - Infill: 20%
- Print 4 x `screw-washer-offset.stl`
    - Note that this part is meant to be a more robust replacement for the rack nut washers that come with the rails and allow for fully tightening the screws. It also shifts the rails up by 1.35mm (at least for the Rosewill case I am using this was required to create clearance for stacking on top of other 4U cases).
    - The `screw-washer.stl` is an alternative washer with no offset.
- Mount the rail-adapters to the rear end of each rail with an M4 nut and M4 x 14mm screw.
- The front of each rail can be attached to the regular metal ear bracket that comes with the TC-RAILs
- Insert 2 M4 nuts into EACH rail adapter in the nut pockets.
- Screw the rail onto your rack using the countersunk screws that come with the TC-RAILs.

