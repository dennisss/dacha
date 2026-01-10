# Helper to verify the equation to use in the 'calculate_backplane_resistance' function
# in jbod_tester.rs

from sympy import symbols, simplify, Add, degree

r_load, v1, gnd1, vcc = symbols('r_load, v1, gnd1, vcc')

# v = i*r
# r = v/i
# i = v/r

v_load = v1 - gnd1
v_load = symbols('v_load')

v_backplane = vcc - v_load

i = v_load / r_load

r_backplane = v_backplane / i

print(r_backplane)

print(simplify(r_backplane))

