// Writen by Google Gemini
// https://gemini.google.com/app/e3024bfb725e3bb4

/**
 * CORE ALGORITHM
 * * Mathematical derivation:
 * t[i] = t[0] + i*d[0] + (i*(i-1)/2)*add
 * t[i] = (add/2)*i^2 + (d[0] - add/2)*i + t[0]
 * * This is a quadratic equation: y = Ax^2 + Bx + C
 * Where: y = time, x = index
 * * We solve for A, B, C using Least Squares Regression.
 * Then map back:
 * add = 2A
 * duration[0] = B + A
 * time[0] = C
 */
export function approximateCurve(times) {
    const n = times.length;
    if (n < 3) return null; // Not enough data for a curve

    // Sums for Normal Equation Matrix (3x3)
    let sx = 0, sx2 = 0, sx3 = 0, sx4 = 0;
    let sy = 0, sxy = 0, sx2y = 0;

    for (let i = 0; i < n; i++) {
        const x = i;
        const y = times[i];
        const x2 = x * x;

        sx += x;
        sx2 += x2;
        sx3 += x2 * x;
        sx4 += x2 * x2;

        sy += y;
        sxy += x * y;
        sx2y += x2 * y;
    }

    // Matrix M * [A, B, C] = R
    // [ sx4 sx3 sx2 ] [ A ]   [ sx2y ]
    // [ sx3 sx2 sx  ] [ B ] = [ sxy  ]
    // [ sx2 sx  n   ] [ C ]   [ sy   ]

    // Solve using Cramer's Rule or Gaussian elimination. 
    // Since it's 3x3, we can hardcode the determinant method for simplicity.
    const m = [
        [sx4, sx3, sx2],
        [sx3, sx2, sx],
        [sx2, sx, n]
    ];
    const r = [sx2y, sxy, sy];

    const det = (m) => {
        return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) -
            m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) +
            m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    };

    const D = det(m);

    // Matrix for A (replace col 0 with R)
    const mA = [[r[0], m[0][1], m[0][2]], [r[1], m[1][1], m[1][2]], [r[2], m[2][1], m[2][2]]];
    const A = det(mA) / D;

    // Matrix for B (replace col 1 with R)
    const mB = [[m[0][0], r[0], m[0][2]], [m[1][0], r[1], m[1][2]], [m[2][0], r[2], m[2][2]]];
    const B = det(mB) / D;

    // Matrix for C (replace col 2 with R)
    const mC = [[m[0][0], m[0][1], r[0]], [m[1][0], m[1][1], r[1]], [m[2][0], m[2][1], r[2]]];
    const C = det(mC) / D;

    // Map back to our specific parameters
    return {
        add: 2 * A,
        duration0: B + A,
        time0: C
    };
}

/**
 * Splits an array into N chunks, processes each with function f,
 * and concatenates the results.
 *
 * @param {Array} inputArr - The source array.
 * @param {number} n - The number of chunks to split into.
 * @param {Function} f - A function that accepts an array (chunk) and returns an array.
 * @returns {Array} - The concatenated result of all transformed chunks.
 */
export function processChunks(inputArr, n, f) {
    // Guard clause for invalid N
    if (n <= 0) return [];

    const length = inputArr.length;
    const results = [];

    for (let i = 0; i < n; i++) {
        // Calculate start and end indices using linear interpolation.
        // This automatically distributes the "remainder" items evenly.
        const start = Math.floor((i * length) / n);
        const end = Math.floor(((i + 1) * length) / n);

        const chunk = inputArr.slice(start, end);

        // Call function f and capture the result
        const transformed = f(chunk);

        results.push(transformed);
    }

    // Flatten the array of arrays into a single array
    return results.flat();
}
