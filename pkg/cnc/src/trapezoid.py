"""
Run this script and it will print out the quadratic equation used in
linear_motion_constraints.rs.

Known Inputs:
- 'acceleration'
- 'start_speed'
- 'distance': target total distance traveled
- 'k': extra time needed to be spent decelerating compared to accelerating.

We want to find:
- 'x' which is the amount of time spent ramping up from the start speed to the peak speed.
- 'x + k' will be the amount of time spent ramping down from the peak speed to the end speed.

We must solve 'x' under the constraint that we travel exactly 'distance' over the time
interval 'x + (x + k)'
"""

from sympy import symbols, simplify, Add, degree

def calculate_distance(start_velocity, acceleration, time):
    """Calculated distance traveled given some start velocity and constant acceleration."""
    return ((acceleration / 2) * (time**2)) + (start_velocity * time)

acceleration, start_speed, d, x, k = symbols('acceleration,start_speed,distance,x,k')

# First we will accelerate up to the peak speed.
d_up = calculate_distance(start_speed, acceleration, x)

# Then decelerate down to the final speed.
peak_speed = start_speed + acceleration * x
d_down = calculate_distance(peak_speed, -acceleration, x + k)

# distance = d_up + d_down
# 0        = d_up + d_down - d
expr = simplify(d_up + d_down - d)

print(expr)
