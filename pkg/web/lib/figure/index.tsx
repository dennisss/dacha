import React from "react";
import { Tooltip, TooltipData, TooltipDataContainer } from "./tooltip";
import { Rect, Point, FigureOptions, EntityKind, LineGraphEntity, CircleEntity, LineEntity } from "./types";

export interface FigureProps {
    options: FigureOptions,

    // NOTE: The passed in point may not be in the axis limits (e.g. if the margin was clicked)
    onClick?: (p: Point) => void,
}

interface FigureState {
    // Dimensions of the entire canvas.
    canvas_height: number | null,
    canvas_width: number | null,

    // Relative to the top-left corner of the canvas, the location of the coordinate system in which we will plot user points.
    graph_rect: Rect | null,
}

export class Figure extends React.Component<FigureProps, FigureState> {
    _root: React.RefObject<HTMLDivElement> = React.createRef();
    _canvas: React.RefObject<HTMLCanvasElement> = React.createRef();
    _canvas_wrap: React.RefObject<HTMLDivElement> = React.createRef();

    _ctx: CanvasRenderingContext2D;

    // If the user's mouse is hovering over the graph, then this will be the mouse position in the canvas coordinate system. 
    _mouse_canvas_pos?: Point = null;

    // Since we re-draw the frame on componentDidUpdate, we update the tooltip in a second pass after that.
    _tooltip_data = new TooltipDataContainer();

    constructor(props: FigureProps) {
        // TODO: Subscribe to window resize events.
        super(props);

        this.state = {
            canvas_height: null,
            canvas_width: null,
            graph_rect: null,
        };
    }

    componentDidMount() {
        this._ctx = this._canvas.current.getContext('2d');

        // TODO: Should periodically re-run this in case the component was resized.

        let rect = this._canvas_wrap.current.getBoundingClientRect();

        let options = this.props.options;

        let x_margin = options.margin.left + options.margin.right;
        let y_margin = options.margin.top + options.margin.bottom;

        let canvas_height = -1;
        let canvas_width = -1;

        // NOTE: This is NOT the final aspect ratio. It is just what is used for sizing if one of the height/width dimensions is not explicitly set.
        let aspect_ratio = (options.aspect_ratio || 1) * (
            (options.x_axis.range.max - options.x_axis.range.min) /
            (options.y_axis.range.max - options.y_axis.range.min)
        );

        if (options.height) {
            canvas_height = rect.height;

            if (!options.width) {
                canvas_width = Math.round(aspect_ratio * (canvas_height - y_margin)) + x_margin;
            }
        }

        if (options.width) {
            canvas_width = rect.width;

            if (!options.height) {
                canvas_height = Math.round((1 / aspect_ratio) * (canvas_width - x_margin)) + y_margin;
            }
        }

        this.setState({
            canvas_height,
            canvas_width,
            graph_rect: {
                x: options.margin.left,
                width: canvas_width - x_margin,
                y: options.margin.top,
                height: canvas_height - y_margin
            }
        }, () => {
            this._draw_frame();
        });
    }

    componentDidUpdate() {
        // TODO: Re-run the sizing calculation.

        this._draw_frame();
    }

    _to_canvas_pt(pt: Point): Point {
        let options = this.props.options;

        let x_range = options.x_axis.range;
        let y_range = options.y_axis.range;

        let x = ((pt.x - x_range.min) / (x_range.max - x_range.min)) * this.state.graph_rect.width + this.state.graph_rect.x;

        let y = ((pt.y - y_range.min) / (y_range.max - y_range.min)) * this.state.graph_rect.height;
        // Inverted since position 'y' is facing downward in canvases.
        // TODO: Don't use the 'margin' here. Prefer to base this on the graph_rect.
        y = this.state.canvas_height - options.margin.bottom - y;

        return {
            x: x,
            y: y
        };
    }

    // Converts from canvas (pixel) space to axis space.
    _from_canvas_pt(pt: Point): Point {
        let options = this.props.options;

        let x_range = options.x_axis.range;
        let y_range = options.y_axis.range;
        let graph_rect = this.state.graph_rect;

        let x = ((pt.x - graph_rect.x) / graph_rect.width) * (x_range.max - x_range.min) + x_range.min;

        let y = (pt.y - graph_rect.y) / graph_rect.height;
        y = 1 - y;
        y = y * (y_range.max - y_range.min) + y_range.min;

        return {
            x: x,
            y: y
        };
    }

    _draw_frame() {
        let opts = this.props.options;

        this._ctx.save();

        this._ctx.clearRect(0, 0, this.state.canvas_width, this.state.canvas_height);

        this._ctx.font = opts.font.style;

        this._ctx.strokeStyle = '#ccc';
        this._ctx.lineWidth = 1;

        // XXX: Populating x ticks here.

        opts.x_axis.ticks.map((tick) => {
            if (tick.value < opts.x_axis.range.min || tick.value > opts.x_axis.range.max) {
                return;
            }

            let x_canvas = this._to_canvas_pt({ x: tick.value, y: NaN }).x;

            // Make the lines sharp.
            x_canvas = Math.round(x_canvas + 0.5) - 0.5;

            this._ctx.beginPath();
            this._ctx.moveTo(x_canvas, this.state.graph_rect.y);

            let y2 = this.state.graph_rect.y + this.state.graph_rect.height;
            this._ctx.lineTo(x_canvas, y2);
            this._ctx.stroke();

            let label = tick.label;
            let dims = this._ctx.measureText(label);

            this._ctx.fillText(label, x_canvas - (dims.width / 2), y2 + opts.font.size + 4);
        });

        opts.y_axis.ticks.map((tick) => {
            let y_canvas = this._to_canvas_pt({ x: NaN, y: tick.value }).y;
            y_canvas = Math.round(y_canvas + 0.5) - 0.5;

            this._ctx.beginPath();
            this._ctx.moveTo(this.state.graph_rect.x, y_canvas);
            this._ctx.lineTo(this.state.graph_rect.x + this.state.graph_rect.width, y_canvas);
            this._ctx.stroke();

            let dims = this._ctx.measureText(tick.label);

            let h = dims.actualBoundingBoxAscent + dims.actualBoundingBoxDescent;

            this._ctx.fillText(tick.label, this.state.graph_rect.x - dims.width - 10, y_canvas + h / 2);
        })

        let tooltip: TooltipData = {
            position: { x: 0, y: 0 },
            right_align: false,
            x_value: '',
            lines: []
        }

        opts.entities.map((entity) => {
            if (entity.kind == EntityKind.LineGraph) {
                this._draw_line_graph(entity, tooltip);
            } else if (entity.kind == EntityKind.Circle) {
                this._draw_circle(entity);
            } else if (entity.kind == EntityKind.Line) {
                this._draw_line(entity);
            } else {
                throw new Error("Don't know how to draw entity");
            }
        });

        if (tooltip.lines.length == 0) {
            tooltip = null;
        }

        this._tooltip_data.update(tooltip);

        this._ctx.restore();
    }

    _draw_line_graph(line: LineGraphEntity, tooltip: TooltipData) {
        if (line.width === 0) {
            return;
        }

        this._ctx.strokeStyle = line.color;
        this._ctx.fillStyle = line.color;
        this._ctx.lineWidth = line.width || 1;

        this._ctx.beginPath();
        this._ctx.rect(this.state.graph_rect.x, this.state.graph_rect.y, this.state.graph_rect.width, this.state.graph_rect.height);
        this._ctx.clip();

        this._ctx.beginPath();

        let closest_graph_pt = null;
        let closest_distance = 10; // Must be within 10 pixels to allow a match at all.

        let last_x = null;

        line.data.map((graph_pt) => {
            let pt = this._to_canvas_pt(graph_pt);

            if (last_x == null || (line.max_interpolation_gap && Math.abs(last_x - graph_pt.x) > line.max_interpolation_gap)) {
                this._ctx.moveTo(pt.x, pt.y);
            } else {
                this._ctx.lineTo(pt.x, pt.y);
            }

            last_x = graph_pt.x;

            // TODO: Also require a minimum y match.
            if (this._mouse_canvas_pos !== null) {
                let distance = Math.abs(pt.x - this._mouse_canvas_pos.x);
                if (distance < closest_distance) {
                    closest_distance = distance;
                    closest_graph_pt = graph_pt;
                }
            }
        });
        this._ctx.stroke();

        if (closest_graph_pt != null) {
            let pt = this._to_canvas_pt(closest_graph_pt);

            this._ctx.beginPath();
            this._ctx.ellipse(pt.x, pt.y, 3, 3, 0, 0, 2 * Math.PI);
            this._ctx.fill();


            // NOTE: We assume that all the lines are aligned to the same x interval, so we just pick the x position based on the first point.
            if (tooltip.lines.length == 0) {

                // NOTE: The rounding here is meant to reduce the update rate.
                tooltip.position = {
                    x: Math.round(this._mouse_canvas_pos.x) + 20,
                    y: Math.round(this._mouse_canvas_pos.y) + 20
                };

                tooltip.x_value = (this.props.options.x_axis.renderer)(closest_graph_pt.x);
            }

            tooltip.lines.push({
                label: line.label,
                y_value: (this.props.options.y_axis.renderer)(closest_graph_pt.y),
                color: line.color
            });
        }
    }

    _draw_circle(circle: CircleEntity) {
        let ctx = this._ctx;
        ctx.fillStyle = circle.color;

        let pt = this._to_canvas_pt(circle.center);

        ctx.beginPath();
        ctx.arc(pt.x, pt.y, circle.radius, 0, 2 * Math.PI);
        ctx.fill();
    }

    _draw_line(line: LineEntity) {
        let ctx = this._ctx;
        ctx.strokeStyle = line.color;
        ctx.lineWidth = line.width;

        ctx.beginPath();

        // TODO: nicely align the points to pixel coordinates (if x or y delta is very low).

        let p1 = this._to_canvas_pt(line.start);
        ctx.moveTo(p1.x, p1.y);

        let p2 = this._to_canvas_pt(line.end);
        ctx.lineTo(p2.x, p2.y);

        ctx.stroke();
    }

    _draw_hover_pointer() {

    }

    _on_mouse_move = (e: React.MouseEvent<HTMLDivElement, MouseEvent>) => {
        let canvas_rect = this._canvas.current.getBoundingClientRect();
        this._mouse_canvas_pos = { x: e.clientX - canvas_rect.x, y: e.clientY - canvas_rect.y };


        // TODO: Debounce me.
        this._draw_frame();
    }

    _on_mouse_out = () => {
        this._mouse_canvas_pos = null;
        this._draw_frame();
    }

    _on_click = () => {
        if (!this.props.onClick) {
            return;
        }

        let pt = this._from_canvas_pt(this._mouse_canvas_pos);
        this.props.onClick(pt);
    }

    render() {
        let options = this.props.options;
        let outer_sizing = { width: options.width, height: options.height };

        // TODO: Don't update the canvas as it will end up having dynamic width/height.
        return (
            <div ref={this._root}
                style={{ fontSize: 0, position: 'relative', ...outer_sizing }}
                onMouseMove={this._on_mouse_move}
                onMouseOut={this._on_mouse_out}
            >
                <div ref={this._canvas_wrap} style={{ overflow: 'hidden', ...outer_sizing }} onClick={this._on_click}>
                    <canvas
                        ref={this._canvas}
                        width={this.state.canvas_width} height={this.state.canvas_height}
                        style={{ cursor: 'pointer' }} />
                </div>

                <Tooltip data={this._tooltip_data} />
            </div>
        );
    }
};