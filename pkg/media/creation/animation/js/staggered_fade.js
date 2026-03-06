// Written by Google Gemini
// https://gemini.google.com/app/3a4b20752af59d86

/**
 * Calculates the alpha (opacity) of a specific object based on a global transition value.
 *
 * @param {number} i - The index of the object (0 to total_objects - 1).
 * @param {number} total_objects - The total number of N objects.
 * @param {number} t - The global transition value (0.0 to 1.0).
 * @param {number} overlap_threshold - At what percentage of the previous item's animation should this one start? (e.g., 0.5 for 50%).
 * @returns {number} - The calculated alpha value for object i (0.0 to 1.0).
 */
export function getObjectAlpha(i, total_objects, t, overlap_threshold = 0.5) {
    // 1. Calculate the duration of a single object's fade relative to the total time (0-1).
    // The logic is: TotalTime = (Duration) + (N-1) * (Duration * overlap_threshold)
    // Solving for Duration (k): k = 1 / (1 + (N-1) * overlap_threshold)
    const k = 1 / (1 + (total_objects - 1) * overlap_threshold);

    // 2. Calculate the start time for this specific object
    const start_time = i * k * overlap_threshold;

    // 3. Map the global 't' to the local object's progress
    // If t is before start_time, result is negative. If after duration, result is > 1.
    let alpha = (t - start_time) / k;

    // 4. Clamp the result between 0 and 1
    if (alpha < 0) alpha = 0;
    if (alpha > 1) alpha = 1;

    return alpha;
}