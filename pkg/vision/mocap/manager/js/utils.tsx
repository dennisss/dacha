import * as THREE from 'three';

/**
 * Converts a stringified u64 decimal integer to a base32 string.
 * @param {string} idStr - The stringified integer (e.g., "1234567890123456")
 * @returns {string | null}
 */
export function entity_id_to_string(idStr) {
    let num = BigInt(idStr);
    const ENCODE_MAP = "0123456789abcdefghjkmnpqrstvwxyz";
    let out = "";

    // Extract the first 4 bits (0b1111)
    let firstCode = Number(num & 15n);
    num >>= 4n;

    if (firstCode < 10) {
        firstCode |= (1 << 4);
    }

    out += ENCODE_MAP[firstCode];

    // Extract the remaining 60 bits in twelve 5-bit chunks (13 chars total)
    for (let i = 0; i < 12; i++) {
        const code = Number(num & 31n); // 0b11111
        num >>= 5n;
        out += ENCODE_MAP[code];
    }

    if (!/^[a-zA-Z]/.test(out)) {
        return null;
    }

    return out;
}


// Given a list of VectorProtos, this will center then.
export function center_points(points) {
    if (!points || points.length === 0) return [];

    points = points.map((p) => new THREE.Vector3(...p.values));

    const centroid = new THREE.Vector3();
    for (let i = 0; i < points.length; i++) {
        centroid.add(points[i]);
    }
    centroid.divideScalar(points.length);

    points = points.map((p) => p.clone().sub(centroid));

    points = points.map((p) => { return { values: [p.x, p.y, p.z] }; });

    return points;
}