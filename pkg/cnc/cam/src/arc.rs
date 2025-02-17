/*
Doing arc interpolation:

- Keep accumulating points while they fit either a linear or arc model

- Arc model:
    Variables:
        - 'x'
        - 'y'
        - 'r'
    For a point:
        Error = sqrt((x_i - x)^2 + (y_i - y)^2) - r

        So need the derivative of this w.r.t. 'x', 'y', 'r'


- In an arc:
    - All points equal distance from a center point
    - (x_i - x)^2 + (y_i - y)^2 = r^2

*/
