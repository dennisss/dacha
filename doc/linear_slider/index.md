

Parts:

- 1 of each 3d printed part
    - Add M3 short heatset inserts into the carriage bracket and the idler tensioner
- 1 MGN12H rail of length N mm
- 1 Aluminum extrusion of length >= N mm


Screws:

- Attaching Rail to Aluminum Extrusion
    - >= 3 x M3 T-Nut
    - >= 3 x M3 8mm socket head screws
- Attaching motor
    - 4 x M3 8mm screws
- Attaching brackets to extrusion
    - 4 x M3 T-Nut
    - 4 x M3 8mm socket head screws
- Attaching carriage bracket to carriage
    - 4 x M3 5mm socket head screws
- Tensioning
    - 1 x M3 nut
    - 1 x M3 25mm screw
- Attaching Idler
    - 2 x washers
    - >= 26mm screw
- Attaching belt clamp
    - M3 x 8mm screw

Magnets:

- 1/4inch to mm is 6.35
- 3/16inch to mm is 4.7625
- 1/8inch to mm is 3.17500
- 1/16inch to mm is 1.5875

Pendulum

- 55mm M5 screw
- 3 x M5 hex nuts
- 2 x M3 10mm screws for clamping the rod
- 4 x M3 8mm screws for attaching the bracket to the carriage.
- Blue thread lock for the M5 bolt.
- 6mm diameter shaft.

odrive uses 6mm diameter x 5mm magnets


Inverted pendulum modeling:

- State variables:
    - X position of the slider
    - X velocity of the slider

- Inputs
    - 

- Friction
    - `torque_friction = -b * angular_velocity`


- Known weight of the rod
    - Rod always experiencing a down

- Depending on the current angle 


