import { Point } from "./types";

export function clean_point(v: any): Point {
    return {
        x: v.x || 0,
        y: v.y || 0
    }
}