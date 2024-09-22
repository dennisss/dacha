

export function shallow_copy<T extends object>(object: T): T {
    let out = {};
    Object.assign(out, object);
    return out as T;
}

export function deep_copy<T extends object>(object: T): T {
    return JSON.parse(JSON.stringify(object));
}

// For building Array.sort comparison functions.
export function compare_values(a: any, b: any): number {
    if (a < b) {
        return -1;
    }

    if (b > a) {
        return 1;
    }

    return 0;
}

export function extend(base: any, diff: any): any {
    let v = shallow_copy(base);
    Object.assign(v, diff);
    return v;
}