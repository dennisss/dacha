// Written by Gemini
// https://gemini.google.com/app/40953b161cee1f75

export function getRayToRectIntersection(width, height, angle) {
    const halfWidth = width / 2;
    const halfHeight = height / 2;

    const c = Math.cos(angle);
    const s = Math.sin(angle);

    // Find the scaling factor to reach the edge of a 1x1 unit square
    // This is the key part: 1 / max(abs(cos), abs(sin))
    const r = 1 / Math.max(Math.abs(c), Math.abs(s));

    // The (x, y) position on the unit square's edge
    const x_norm = c * r;
    const y_norm = s * r;

    // Scale the normalized point by the rectangle's dimensions
    const x = x_norm * halfWidth;
    const y = y_norm * halfHeight;

    return { x, y };
}