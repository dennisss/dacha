
export function round_digits(num: number, digits: number): number {
    let scale = Math.pow(10, digits);
    return Math.round(num * scale) / scale;
}

export function format_bytes_size(value: number): string {
    const MULTIPLIER = 1024;
    const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];

    let unit_index = 0;
    while (unit_index + 1 < UNITS.length && value >= MULTIPLIER) {
        value /= MULTIPLIER;
        unit_index += 1;
    }

    let num = round_digits(value, 2)

    return `${num} ${UNITS[unit_index]}`;
}

// Formats a google.protobuf.Duration object as a human readable string.
export function format_duration_proto(value: any): string {
    let secs = (value['seconds'] || 0) * 1;
    secs += (value['nanos'] || 0) / 1000000000;

    let out = '';

    const SECS_PER_HOUR = 60 * 60;
    if (secs > SECS_PER_HOUR) {
        let v = Math.floor(secs / SECS_PER_HOUR);
        secs -= v * SECS_PER_HOUR;
        out += `${v} hour${v != 1 ? 's' : ''} `;
    }

    const SECS_PER_MINUTE = 60;
    if (secs > 60) {
        let v = Math.floor(secs / SECS_PER_MINUTE);
        secs -= v * SECS_PER_MINUTE;
        out += `${v} minute${v != 1 ? 's' : ''} `;
    }

    if (secs > 0 || out.length == 0) {
        out += `${secs} second${secs != 1 ? 's' : ''}`;
    }

    return out.trim();
}