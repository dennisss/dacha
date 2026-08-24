
# Optical Motion Capture : Lens 

This page describes sourcing guidance for a lens. The lens additionally needs to be paired with:

- A lens holder to attach to the camera board
    - Most of our boards support 20mm screw spacing M12 lens holders with up to M2 sized screws
    - You also need a way of rigidly locking the lens in place in the lens holder.
- An 850nm IR bandpass filter

## Recommendations

These are the recommended lens to buy if you want a good off the shelf option (these come with 850nm IR bandpass pre-installed and the price includes the lens holder). You can make higher performance assemblies with a bit more effort, but these will form fairly well for most people:

- [4.35mm Focal Length](https://www.aliexpress.us/item/3256804658808702.html) (recommended)
    - Narrower FOV better for long range.
    - 1/2.3" image circle
- [3.6mm Focal Length](https://www.aliexpress.us/item/2251832686546887.html) 
    - Wider than average FOV
    - 1/2.5" image circle

## Physical Dimensions

Keep in mind that the physical size of the lens you use is constrained by the rest of the hardware sandwidth design and needs to align with the following requirements:

- TTL: ~22mm (+/- 0.5mm) (not including the IR filter)
    - This is the distance from the image sensor to the farthest tip of the Lens when focused.
    - Note: For spacing calculations, you also need to add ~0.15mm since the IR filter glass shifts the position of the focal plane (so check if TTL is measured with or without any builtin IR filter).
- Diameter:
    - 14mm (<15mm max) : When using the regular IR LED ring
    - 16mm (<17mm max) : When using a wide angle IR LED ring.

## General Lens Guidance

We need to pick a lens to go with the camera sensor to cover a good amount of space in our room without being so wide that objects far appear very small. Also note that FOVs over 70 degrees start having distortion that is more computationally expensive to deal with so is ideally avoided. If using an IR light ring The focal length of the lens must also be compatible with the field of view of the LEDs.

Also note that the Lens MUST NOT have a standard built in IR filter:

- Most lenses regardless of whether or not they say the word "IR" have a IR filter that blocks out light above 650nm.
    - NIR / "No IR" is one form of wording we are looking for but this is super inconsistent between suppliers as most don't bother mentioning any details about the IR filter unless you ask them directly.
    - When in doubt, ask the seller whether or not the lens contains any IR filters.
- The recommended lens has a built in '850nm bandpass filter'
    - This wording is a good sign that it doesn't have the regular 650nm low pass filter since otherwise these statements would be conflicting.
- If you do find a lens with absolutely zero IR filter, you will need to get a separate bandpass 850nm filter glass and glue it to the back of the lens (or in front of the lens if you have a large enough piece of glass and block out any light going into the lens from other angles).

Once you find a lens + 850nm band pass filter combo, make sure to find the exact band width of the filter. Usually either in the filter description page or if you ask the supplier, they will give you a graph of frequency vs 'transmission %'. For the ELP lens, the graph has a peak of >90% transmission in the `830 - 870 nm` range. This will be important for validating compatibility with the LEDs. In general, you want as narrow of a band as possible that also fits the majority of your LED light without filtering.

## Materials

The material that the lens and lens holder are made of will determine how thermally stable they are (better materials have lower thermal expansion so less positional accuracy drift over time):

- Lens
    - All glass element lenses are best (most cheap lens use some glass + some plastic elements).
- Lens Holder
    - No name plastics are the worst.
    - Aluminum is good.
    - LCP (plastic) is great.

## Assembly

Make sure to apply `Nyogel 767A` damping grease to the lens thread to minimize motion over time.

## Optimal Components

These are the best and the best components you can get for roughly the same price as the recommended lens but will require more manual effort to assemble

- Lens
    - [3.9mm focal length](https://www.digikey.com/en/products/detail/edatec/ED-LENS-M12-230390-08/25659396) normal FOV
    - [2.7mm focal length](https://www.digikey.com/en/products/detail/edatec/ED-LENS-M12-230270-08/25659394) wide FOV
    - These are good because they have very low distortion and are high resolution (MTF wise)
    - They don't come with any filters
- IR Filter
    - 0.5mm thickness. 9mm diameter. 45nm FWHM
    - Glue to the back of the lenses with 3 small drops of `NOA 68T` ABOVE THE FILTER (the filter should ideally lay flat against the lens).
- Lens Holder
    - LCP 10mm height, 20mm spacing
    - Screws: M1.6 x 6mm : https://www.mcmaster.com/96817A401/


