export function encode_be_u32(value: number, buffer: Uint8Array) {
    if (buffer.length != 4) {
        throw new Error("Expected a buffer of size 4");
    }

    if (value >= Math.pow(2, 32) || Math.trunc(value) !== value) {
        throw new Error("Value can't be represented as a 32-bit uint");
    }

    for (let i = 0; i < buffer.length; i++) {
        buffer[buffer.length - i - 1] = value & 0xff;
        value >>= 8;
    }
}

export function decode_be_u32(buffer: Uint8Array): number {
    if (buffer.length != 4) {
        throw new Error("Expected a buffer of size 4");
    }

    let out = 0;
    for (let i = 0; i < buffer.length; i++) {
        out = (out << 8) | buffer[i];
    }

    return out;
}

export function encode_utf8(value: string): number[] {
    let out = [];

    for (let i = 0; i < value.length; i++) {
        let code = value.charCodeAt(i);

        if (code <= 0x7f) {
            out.push(code);
        } else if (code <= 0x7FF) {
            // TODO:
        }

    }

    return out;
}

export function decode_utf8(buffer: Uint8Array): string {
    let out = "";

    let i = 0;
    while (i < buffer.length) {
        let code = buffer[i];
        i += 1;

        // Process first byte: Count and remove leading 1's
        let extra_bytes = 0;
        while ((code & (1 << 7)) != 0) {
            extra_bytes += 1;
            code <<= 1;
        }
        code >>= extra_bytes;

        for (let j = 0; j < extra_bytes; j++) {
            let b = buffer[i];
            i += 1;
            if (b >> 6 != 0b10) {
                throw new Error("Invalid extra code byte");
            }

            code = (code << 6) | (b & 0b111111);
        }

        out += String.fromCharCode(code);
    }

    return out;
}

// NOTE: This is very loose on which syntax it will accept.
export function decode_header_block(value: string): Map<string, string> {
    let out = new Map();

    value.split("\r\n").map((line) => {
        if (line.length == 0) {
            return;
        }

        let parts = line.split(":");
        let name = parts[0].trim();
        let value = parts[1].trim();
        out.set(name, value);
    });

    return out;
}
