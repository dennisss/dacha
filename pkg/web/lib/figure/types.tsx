export interface FigureOptions {

    // Raw CSS width/height to use for the graph container.
    // 
    // At least one of these must be specified.
    // If one is missing, it will calculated via aspect_ratio.
    width?: number | string;
    height?: number | string;

    max_height?: number;

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

export type Entity = LineGraphEntity | CircleEntity | LineEntity | ImageEntity | PathEntity;

export enum EntityKind {
    Line,
    LineGraph,
    Circle,
    Image,
    Path
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

export interface PathEntity {
    kind: EntityKind.Path;
    color: string;
    width: number;
    points: Point[];
    closed: boolean;
}

export interface LineGraphEntity {
    kind: EntityKind.LineGraph;

    label: string;
    color: string;

    width?: number;

    // If present, the maximum 'x' distance between two poitns for drawing a line.
    max_interpolation_gap?: number;

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

export interface ImageEntity {
    kind: EntityKind.Image;
    image: HTMLImageElement;

    // Rectangle in the figure coordinate space. rect.x and rect.y are the bottom-left corner of the image.
    rect: Rect;
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
