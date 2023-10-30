/*
This requires a Point2 type:
- Must be able to order the points (and test for equality)
    - First based on y, then x
- Must be able to compute intersections.
    - How do we define them as intersecting?
        - Means that we are within one unit of the line (although a challenge is that this could introduce intersections with other lines)?

For a point to be inside a line segment, it must:
- Either:
    - Be equal to one of the end points
    - Be a dir away from one of the endpoints
- Both endpoints should not be inside the line formed by the point and the other endpoint.

For line intersections:
- Check dot product is >= 0 at both endpoints.

What I want to ensure:
- When I produce an intersection,



(0, 0) to (10, 0)
and
(0, 1) to (11, -1)
^ Intersects at (5.5, 0)


a dot b = |a| |b| cos(theta)


compute two dot products.
- If sum is < total distance, then


a_1*x + b_1*y = c_1

a_2*x + b_2*y = c_2



y = m*x + b

(y - b) / m

What is at y = N?
What is at y = (N + 1)


If I were to speed from y = [N, N+1), what range of x values could I expect.

Given y, find the closest x.



position = base + (t * dir)
x = b_x + (t * d_x)
y = b_y + (t * d_y)

(x - b_x) / d_x = (y - b_y) / d_y

(x - b_x) * dy = (y - b_y) * d_x

(x * dy) - (b_x * dy) = (y * d_x) - (b_y * d_x)

(x * dy) - (y * d_x) =  (b_x * dy) - (b_y * d_x)




(0, 1) to (100, -1)

y=0

x * (-2) = -(1 * 100)
x * -2 = -100
x = 50

Assumption is that the y is in the bbox so no


*/

fn isolve(a: &MatrixStatic<i64, U2, U2>, b: &Vector2i64) -> Option<Vector2i64> {
    let det: i64 = a[(0, 0)] * a[(1, 1)] - a[(0, 1)] * a[(1, 0)];
    if det == 0 {
        return None;
    }

    let adj =
        MatrixStatic::<i64, U2, U2>::from_slice(&[a[(1, 1)], -a[(0, 1)], -a[(1, 0)], a[(0, 0)]]);

    let v = (adj * b) / det;
    Some(v)
}

// Integer intersection of two lines
/*
let point = {
    let (a1, b1, c1) = self.standard_form_coeffs();
    let (a2, b2, c2) = other.standard_form_coeffs();

    let mat_a = MatrixStatic::<i64, U2, U2>::from_slice(&[a1, b1, a2, b2]);
    let mat_b = Vector2i64::from_slice(&[c1, c2]);

    match isolve(&mat_a, &mat_b) {
        Some(v) => v,
        None => return None,
    }
};
*/
