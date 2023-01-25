

/*
Important equations:

acceleration = [constant]

final_velocity = acceleration * delta_time + initial_velocity

final_position = ((acceleration/2) * delta_time^2) + (initial_velocity * delta_time) + initial_position

0 = ((acceleration/2) * delta_time^2) + (initial_velocity * delta_time) + (initial_position - final_position)


We know initial_position, final_position, initial_velocity
=> Want to know


Once we know:
- start_position
- end_position
    - These give us a 'distance'
- start_velocity (start_speed)
- max_end_velocity (max_end_speed)

Variables: RampUpTime, RampDownTime

NOTE: We assume the start velocity is not too high.

cruise_speed = ramp_up_time * acceleration + start_speed.

ramp_up_distance = ((acceleration/2) * ramp_up_time^2) + (start_speed * ramp_up_time)
ramp_down_distance = ((-acceleration/2) * ramp_down_time^2) + (cruise_speed * ramp_down_time)

cruise_distance = (distance - ramp_up_distance - ramp_up)
cruise_time = cruise_distance / cruise_speed

end_speed = cruise_speed - (acceleration * ramp_down_time)



===

ramp_up_time = X
ramp_down_time = X + K

start_speed + X*acceleration

distance = ((acceleration/2) * X^2) + (start_speed * X) + (-acceleration/2) * (X + K)^2 + (start_speed + X*acceleration) * X



(distance, acceleration, X, start_speed, K) = sympy.var("distance,acceleration,X,start_speed,K")
distance = ((acceleration/2) * X**2) + (start_speed * X) + (-acceleration/2) * (X + K)**2 + (start_speed + X*acceleration) * X

0 = X**2*acceleration - K*X*acceleration + 2*X*start_speed + -K**2*acceleration/2 - distance\
0 = X**2*acceleration + (2*start_speed - K*acceleration) * X + -K**2*acceleration/2 - distance


Constraints:
    ramp_up_time >= 0
    ramp_down_time >= 0

    cruise_time >= 0
    cruise_speed <= max_speed
    end_speed <= max_end_speed

Minimize:
    cruise_time + ramp_up_time + ramp_down_time




Approximating the solution
-

It is possible to


There are basically 4 directions in which we can step.
- Try stepping in each direction.
    - If outside the region, optimize for that,

*/
