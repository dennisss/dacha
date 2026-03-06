// Written by Google Gemini
// https://gemini.google.com/app/4a1936db705c51f8
// https://gemini.google.com/app/0448097e5fc98f04

/**
 * Linearly interpolates a value from a sorted array of data points.
 *
 * @param {Array<{time: number, x: number, dx: number}>} data - Array of data points sorted by time.
 * @param {number} targetTime - The time at which to interpolate.
 * @param {string} key - The property key to interpolate ('x' or 'dx').
 * @returns {number|null} - The interpolated value, or null if input is invalid.
 */
export function interpolateValue(data, targetTime, key) {
    // 1. Basic validation
    if (!data || data.length === 0) {
        return null;
    }

    // 2. Handle edge cases (Target time is outside the range of data)
    // If target is before the first point, return the first point's value.
    if (targetTime <= data[0].time) {
        return data[0][key];
    }
    // If target is after the last point, return the last point's value.
    if (targetTime >= data[data.length - 1].time) {
        return data[data.length - 1][key];
    }

    // 3. Find the two surrounding data points (p0 and p1)
    // Since data is sorted, we find the index `i` such that data[i].time <= targetTime < data[i+1].time
    // We can use a binary search for O(log n) efficiency, which is better for large arrays than find().
    let left = 0;
    let right = data.length - 1;
    let index = 0;

    while (left <= right) {
        const mid = Math.floor((left + right) / 2);

        if (data[mid].time === targetTime) {
            return data[mid][key]; // Exact match found
        }

        if (data[mid].time < targetTime) {
            index = mid;      // Possible candidate for lower bound
            left = mid + 1;   // Search right half
        } else {
            right = mid - 1;  // Search left half
        }
    }

    const p0 = data[index];
    const p1 = data[index + 1];

    // 4. Calculate the interpolation
    // Formula: y = y0 + (y1 - y0) * ((t - t0) / (t1 - t0))
    const timeSpan = p1.time - p0.time;

    // Prevent division by zero if two points have the exact same time
    if (timeSpan === 0) {
        return p0[key];
    }

    const ratio = (targetTime - p0.time) / timeSpan;

    return p0[key] + (p1[key] - p0[key]) * ratio;
}

/**
 * Performs linear interpolation on a sorted list of {x, y} points.
 *
 * @param {Array<{x: number, y: number}>} data - List of points sorted by x.
 * @param {number} targetX - The x value to find the corresponding y for.
 * @returns {number|null} The interpolated y value, or null if input is invalid.
 */
export function getInterpolatedY(data, targetX) {
    // 1. Input validation
    if (!data || data.length === 0) return null;
    if (data.length === 1) return data[0].y;

    // 2. Handle out of bounds (Clamp to first/last value)
    // If targetX is smaller than the first point, return the first y
    if (targetX <= data[0].x) return data[0].y;
    // If targetX is larger than the last point, return the last y
    if (targetX >= data[data.length - 1].x) return data[data.length - 1].y;

    // 3. Find the segment (Linear Search)
    // We look for the first point that is greater than our targetX.
    // The segment will be between [i-1] and [i].
    for (let i = 1; i < data.length; i++) {
        if (data[i].x >= targetX) {
            const p0 = data[i - 1]; // Lower bound
            const p1 = data[i];     // Upper bound

            // Avoid division by zero if x values are identical
            if (p0.x === p1.x) return p0.y;

            // Calculate the ratio (0.0 to 1.0)
            const ratio = (targetX - p0.x) / (p1.x - p0.x);

            // Linear Interpolation formula: y0 + ratio * (y1 - y0)
            return p0.y + ratio * (p1.y - p0.y);
        }
    }

    return null; // Should technically be unreachable due to bounds checks
}