# Optical Motion Capture : Markers

The markers should ideally be spheres so that they have an easily identifiable center position. Required characteristics are:

- Large enough to be visible as multiple pixels in the camera
    - I usually use 19mm diameter spheres for general purpose use.
- Retroreflective (most light bounces back to the source)
    - Most objects aren't retroreflective so we will be dimmer in an image so we can identify markers are spots in a well illuminated image that are brighter than everything else.
    - Normally this is achieved with a glass microsphere + metalic base layer paint.
- Size
    - 9mm diameter for finer details like fingers.
    - 14mm diameter is typical for human body points.
    - \>= 19mm diameter for defining bigger rigid bodies (e.g. drones)

Where to buy them?

- Premade ones from [OptiTrack](https://optitrack.com/accessories?cat=markers)
- Other shops available online if you Google search for "mocap markers"

How to make them yourself:

- TODO: Document the 3 ways to do this.

## Active Markers

If you are tracking something that has a battery, then instead of using 'passive markers', you can attach 'active markers' (wide angle IR LEDs) to your object.

Pros:

- Typically longer range tracking compared to passive markers
- Don't need to attach the IR LED ring to your cameras.

Cons:

- Wastes battery power
- Smaller field of view compared to standard markers


LED Options (only need to drive at around <= 100mA):

- https://www.digikey.com/en/products/detail/ams-osram-usa-inc/SFH-4714B-R33/21700203


## Body Suit 

- Band material
    - 3/4" wide Velstretch (~3.6 meters per person)
    - Regular adhesive backed hook velcro (just need to a small amount cut to size to make the velstretch into a wearable 'bracelet').
- 16 x `marker-base.stl` out of ASA
- 16 x 14mm spherical markers
- 16 x M4 10mm button head screws

## Light Source

The light that our markers will reflect and our cameras will emit and observe will be 850nm near infrared light.

Ideally you want your scene to be completely dark and only shine a little bit of light at the scene so that there is a high contrast in an image taken of the scene between the retroreflective markers and everything else. The main issue is that indoor lights and the sun through windows will create a lot of noise in this process if we just look at all light like a regular camera does. The solution will be to filter to only observing IR light which is relatively low intensity from the sun.

850nm (most common in indoor motion capture) and 940nm (common in TV remotes) are the most common frequencies available with abundant hardware support. 940nm is technically better since it emitted less by the sun but the downside is that typical silicon image sensors become increasingly less sensitive to light at higher wavelengths so will be much harder to see in general.
