// This was written by Google Gemini
// https://gemini.google.com/app/a572ba3117ff603c

/**
 * Converts a hex color string and an alpha value into an rgba string.
 * * @param {string} hex - The hex color string (e.g., "#FFF", "000000", "#FF5733").
 * @param {number} alpha - The opacity (0 to 1). Defaults to 1 if not provided.
 * @returns {string} The formatted rgba string (e.g., "rgba(255, 255, 255, 0.5)").
 */
export function hexToRgba(hex, alpha = 1) {
    // Remove the hash at the start if it's there
    let cleanHex = hex.replace('#', '');

    // Handle shorthand hex codes (e.g., "FFF" -> "FFFFFF")
    if (cleanHex.length === 3) {
        cleanHex = cleanHex.split('').map(char => char + char).join('');
    }

    // specific check to ensure valid hex length
    if (cleanHex.length !== 6) {
        throw new Error('Invalid hex color format. Must be 3 or 6 characters.');
    }

    // Parse the r, g, b values
    const r = parseInt(cleanHex.substring(0, 2), 16);
    const g = parseInt(cleanHex.substring(2, 4), 16);
    const b = parseInt(cleanHex.substring(4, 6), 16);

    // Return the formatted string
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/*
// --- Usage Examples ---

// Standard 6-digit hex
console.log(hexToRgba('#00ABCD', 0.5));
// Output: "rgba(0, 171, 205, 0.5)"

// Shorthand 3-digit hex
console.log(hexToRgba('#F00', 0.8));
// Output: "rgba(255, 0, 0, 0.8)"

// No hash in string
console.log(hexToRgba('FFFFFF', 0.2));
// Output: "rgba(255, 255, 255, 0.2)"
*/