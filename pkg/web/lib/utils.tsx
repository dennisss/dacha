

export function shallow_copy<T extends object>(object: T): T {
    let out = {};
    Object.assign(out, object);
    return out as T;
}