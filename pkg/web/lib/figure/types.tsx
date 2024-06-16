export interface FigureOptions {

    // Raw CSS width/height to use for the graph container.
    // 
    // At least one of these must be specified.
    // If one is missing, it will calculated via aspect_ratio.
    width?: number | string;
    height?: number | string;

    // Ratio calculated as pixels_per_y/pixels_per_x.
    // Defaults to 1. 
    aspect_ratio?: number;

    // Space in pixels between the boundary of the canvas and the inner plot.
    // This space is used for drawing axis labels, etc.
    margin: {
        left: number;
        bottom: number;
        right: number;
        top: number;
    };

    font: {
        style: string;
        size: number
    };

    x_axis: Axis;

    y_axis: Axis;

    // Entities to draw (will be drawn in the order they are specified).
    entities: Entity[];
};

export type Entity = LineGraphEntity | CircleEntity | LineEntity;

export enum EntityKind {
    Line,
    LineGraph,
    Circle,
}

export interface Axis {
    range: Range;
    ticks: Tick[];
    renderer: (v: number) => string;
}

export interface Tick {
    value: number;
    label: string
}

export interface LineEntity {
    kind: EntityKind.Line;
    color: string;
    width: number; // in pixels

    start: Point;
    end: Point;
}

export interface LineGraphEntity {
    kind: EntityKind.LineGraph;

    label: string;
    color: string;

    // TODO: Should always be sorted by x coordinate.
    data: Point[];
}

export interface CircleEntity {
    kind: EntityKind.Circle;
    center: Point;
    color: string;

    // NOTE: This radius is in units of pixels.
    radius: number;
}


export interface Rect {
    x: number,
    y: number,
    width: number,
    height: number
};

export interface Range {
    min: number,
    max: number
}

export interface Point {
    x: number,
    y: number
}
