# Optical Motion Capture : Network Switch

This page explains what to look out for when shopping for a network switch for your cameras.

NOTE: The network switch should ideally have enough ports for all the cameras you want to use (+ 1 for your computer). So buy a big enough network switch for any planned future growth (splitting across multiple switches is not good without careful planning).

## Recommended Models

These are recommended models that you can get brand new (you can often also find these used for slightly cheaper on eBay). Models on this list generally should be plug and play and not require any extra setup:

- `MikroTik CRS328-24P-4S+RM`
    - ~$450 new, ~$350 used 
    - Good for ~22 cameras
    - If you buy used, make sure to update to the latest firmware.
    - I have this one and replaced the fans with 4 x `Noctua NF-A4x20 PWM` (12V) fans (you just need a screwdriver to install them) to make it quieter.
- `TP-Link TL-SG1218MPE`
    - $170 new
    - Good for ~12 cameras
- `TP-Link TL-SG105MPE`
    - $90 new
    - Good for ~4 cameras

If you are purely concerned about cost, you can get old enterprise switches off eBay (these may be louder / less efficient and tend to no longer have any more manufacturer support):

- `Aruba S2500-48P-4X L3 POE+`
    - <$100 on eBay.
    - Good for ~20 cameras

## Shopping Guidance

- Switch must support `PoE+` (or `PoE++`). Regular `PoE` is not enough power unless you are using active markers.
- PoE 'power budget' must be at `>= 20W` per camera (actual power usage will be lower, but it will spike during frame captures).
- PTP support in the network switch doesn't matter.
- If you see switches with "SFP" ports, you will need separate adapters to ethernet (or lots of special cables) so you should avoid these. Ok to have one SFP port for connecting to the host computer.
- If you want to stream video frames from 6+ cameras, I'd recommend getting a network switch with a 10Gbps ethernet or SFP port and pairing them with an appropriate adapter to your host computer.

## MikroTik Specifics

Prefer to use RouterOS. That's what we'll be supporting in the software over the long term.