

## Filament sensor cable:

- 1.5mm pitch 5 pin
    - Molex CLIK-Mate
    - Connector Housing: https://www.molex.com/en-us/products/part-detail/5025780500
    - Crimp Termals: https://www.molex.com/en-us/products/part-detail/5025790000
    - Through Hole Receptacle: https://www.molex.com/en-us/products/part-detail/5031590500

## Camera Mount

This is a camera mount for attaching a USB camera (any 38x38mm camera with a 28x28 hole pattern). Originally this was designed to be used with [this Arducam IMX291 board](https://www.amazon.com/gp/product/B07ZRJDTBQ/ref=ppx_yo_dt_b_search_asin_title?ie=UTF8&th=1) which comes with a 100 degree diagonal FOV (90 horizontal, 68 vertical). 

This mounts the camera from the outside of the enclosure since there isn't much space inside the enclosure to capture a full view of the built plate and this helps to avoid overheating of the camera.

**Why is the mount so big?** To avoid reflections off the plastic window getting into the camera, the mount roughly covers the entire window space the camera looks through.

Installation:

- 3d Print
    - 1 x `xl-camera-mount.stl` (requires supports touching build surface. See the picture)
    - 3 x `cable-holder.stl` (use a brim)
- Attach the camera to the camera mount:
    - Insert the camera from the inside of the mount.
    - Fasten with 4 x M2 6mm screws and 4 x M2 nuts.
- Attaching the camera mount to the front door
    - Use 4 x M4 12mm screws (10mm minimum). This will replace the stock M4 8mm screws.
- Route the camera cable upwards along the edge of the door.
    - The `cable-holder.stl` parts can be used to hold down the wire
        - Use 2 cable holders attached to the door using M4 10mm screws (replacing existing M4 8mm screws)
        - Use 1 cable holders attached to the top left of the enclosure using an M3 6mm screw and nut into an empty hole.

There is an optional `fan-lid.stl` that can be printed to mount an extra 30mm 5V fan on top of the camera to prevent overheating. To use it:

- Splice a 30mm fan's power cable into the camera's USB cable.
- Print a `fan-lid.stl`
- Replace camera mounting screws with 4 x M2 8mm screws
- Add 4 x M3 12mm screws and 4 x M3 nuts for the fan
