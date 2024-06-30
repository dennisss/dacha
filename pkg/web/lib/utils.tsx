

export function shallow_copy<T extends object>(object: T): T {
    let out = {};
    Object.assign(out, object);
    return out as T;
}

export function deep_copy<T extends object>(object: T): T {
    return JSON.parse(JSON.stringify(object));
}