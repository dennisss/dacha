// https://gemini.google.com/app/8b85145b6bc00b0e


/**
 * Calculates a point {x, y} on the line defined by p1 and p2 
 * at a specific y-coordinate.
 * * @param {Object} p1 - The first point {x, y}
 * @param {Object} p2 - The second point {x, y}
 * @param {number} targetY - The y-coordinate to solve for
 * @returns {Object|null} - The calculated point {x, y}, or null if the line is horizontal.
 */
export function getPointAtY(p1, p2, targetY) {
    // 1. Handle the edge case of a horizontal line.
    // If y1 == y2, the slope is 0 (or undefined for x-solving), 
    // so we cannot find a unique X for a specific Y unless it covers all X.
    if (p2.y - p1.y === 0) {
        return null;
    }

    // 2. Calculate the ratio (t) of how far targetY is between p1.y and p2.y
    // Formula: (y - y1) / (y2 - y1)
    const t = (targetY - p1.y) / (p2.y - p1.y);

    // 3. Interpolate the X value using that ratio
    // Formula: x = x1 + t * (x2 - x1)
    const x = p1.x + t * (p2.x - p1.x);

    return { x: x, y: targetY };
}

/*
// --- Examples ---

const point1 = { x: 0, y: 0 };
const point2 = { x: 10, y: 10 };

// Example 1: Find X when Y is 5 (midpoint)
const result1 = getPointAtY(point1, point2, 5);
console.log(result1); // Output: { x: 5, y: 5 }

// Example 2: Find X when Y is 2 (20% of the way)
const result2 = getPointAtY(point1, point2, 2);
console.log(result2); // Output: { x: 2, y: 2 }

// Example 3: Extrapolation (finding a point outside the segment)
const result3 = getPointAtY(point1, point2, 15);
console.log(result3); // Output: { x: 15, y: 15 }
*/