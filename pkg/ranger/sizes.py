import sympy

min_distance = 0.05
max_distance = 2
fov_deg = 48.8  # 0.851719956

# We get a good solution at:
# theta = 0.6887 radians (39.46 degrees)
# x = d01 meters

def main():
    theta, x = sympy.symbols('theta,x')
    fov_radians = fov_deg * (sympy.pi / 180)

    print(fov_radians)

    solved = sympy.solve([
        sympy.Eq(sympy.tan(theta), min_distance / x),
        sympy.Eq(sympy.tan(theta + fov_radians), max_distance / x)
    ], theta, x)

    print(solved)
    
main()