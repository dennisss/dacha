
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
export function format_duration_proto(value: any, round_to: TimeUnit = TimeUnit.Second): string {
    let secs = (value['seconds'] || 0) * 1;
    secs += (value['nanos'] || 0) / 1000000000;
    return format_duration_secs(secs, round_to);
}

export enum TimeUnit {
    Second = 'second',
    Minute = 'minute',
    Hour = 'hour'
}

export function format_duration_secs(secs: number, round_to: TimeUnit = TimeUnit.Second): string {
    let out = '';

    const UNITS = [
        { name: TimeUnit.Hour, value: 60 * 60 },
        { name: TimeUnit.Minute, value: 60 },
        { name: TimeUnit.Second, value: 1 }
    ];

    for (let i = 0; i < UNITS.length; i++) {
        let unit = UNITS[i];
        let is_final = unit.name == round_to;

        let v = secs / unit.value;
        if (is_final) {
            v = Math.round(v);
        } else {
            v = Math.floor(v);
            secs -= v * unit.value;
        }

        if (v > 0 || (is_final && out.length == 0)) {
            out += `${v} ${unit.name}${v != 1 ? 's' : ''} `;
        }

        if (is_final) {
            break;
        }
    }

    return out.trim();
}

const NANOS_PER_SECOND = 1000000000;

// Converts a google.protobuf.Timestamp proto object to a number of milliseconds since epoch.
export function timestamp_proto_to_millis(obj: any): number {
    return 1000 * (obj.seconds || 0) + (1000 / NANOS_PER_SECOND) * (obj.nanos || 0)
}
