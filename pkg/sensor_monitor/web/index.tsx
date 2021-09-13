import React from 'react';
import ReactDOM from 'react-dom';


/*
Bundle using:
./node_modules/.bin/webpack -c ./pkg/sensor_monitor/webpack.config.js --watch

Next step features:
- Hover over a point to get all the metrics about it and the exact time.
- Smooth sliding if new data / a new x_range is y_range is present.
- User customizable x and y ranges
- Support breaks between points (e.g. if the interpolation step is too high).
- Support cumulative charts where everything below the line is filled in (with tranparency).
- Clicking on a point should freeze the tooltip at that point.

We'll have an /api/read_data
- Inputs:
    - Data point filter
    - Time range.
- Outputs
    - List of all data sources with names.


Bundle building for containers:
- BundleSpec consists of list of file paths relative to project root.
    - e.g. dacha/target/debug/sensor_monitor
    - Requirements for bundles:
        - Must be able to keep bundles up to date and determine if a bundle is up-to-date quickly (ideally without re-building it.)
    - Things that need to happen to build the bundle:
        - Run webpack
        - Run the cargo build
        - Bundle the files.



*/

interface GraphOptions {
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

};

interface Rect {
    x: number,
    y: number,
    width: number,
    height: number
};

interface Range {
    min: number,
    max: number
}

interface Point {
    x: number,
    y: number
}

interface GraphData {
    x_range: Range,
    y_range: Range,
    points: Point[]
}

interface TooltipData {
    // Position of the tooltip relative to the top-left corner of the canvas.
    position: Point,

    right_align: boolean,

    x_value: string,

    lines: {
        label: string,
        y_value: string,
        color: string
    }[]
}

interface FigureState {
    // Dimensions of the entire canvas.
    canvas_height?: number,
    canvas_width?: number,

    // Relative to the top-left corner of the canvas, the location of the coordinate system in which we will plot user points.
    graph_rect?: Rect,

    tooltip?: TooltipData
}

interface TooltipProps {
    data: TooltipData
}

class Tooltip extends React.Component<TooltipProps> {

    render() {
        let data = this.props.data;

        // TODO: It would be a better experience if this was a fixed width.

        return (
            <div style={{ position: 'absolute', top: data.position.y, left: data.position.x, padding: 5, backgroundColor: '#fff', border: '1px solid #ccc', fontSize: 12 }}>
                <div style={{ fontWeight: 'bold', paddingBottom: 4 }}>
                    {data.x_value}
                </div>
                <div>
                    {data.lines.map((line, i) => {
                        return (
                            <div key={i}>
                                <div style={{ display: 'inline-block', backgroundColor: line.color, width: 10, height: 5 }}></div>
                                <div style={{ display: 'inline-block', minWidth: 60, paddingRight: 4, paddingLeft: 4, fontWeight: 'bold' }}>
                                    {line.label + ':'}
                                </div>
                                <div style={{ textAlign: 'right', display: 'inline-block' }}>
                                    {line.y_value}
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>
        );
    }

}

function round_digits(num: number, digits: number): number {
    let scale = Math.pow(10, digits);
    return Math.round(num * scale) / scale;
}

class Figure extends React.Component<{}, FigureState> {
    _root: HTMLElement;
    _canvas: HTMLCanvasElement;
    _ctx: CanvasRenderingContext2D;

    _options: GraphOptions;

    _x_axis: Range;
    _y_axis: Range;

    // TODO: Should always be sorted by x coordinate.
    _data: Point[];

    // If the user's mouse is hovering over the graph, then this will be the mouse position in the canvas coordinate system. 
    _mouse_canvas_pos?: Point = null;

    state = {
        canvas_height: null,
        canvas_width: null,
        graph_rect: null,
        tooltip: null
    };

    constructor(props) {
        super(props);

        this._options = {
            margin: {
                left: 40,
                bottom: 20,
                top: 6,
                right: 2
            },
            font: {
                style: '14px "Noto Sans"',
                size: 14
            }
        };

        this._x_axis = { min: 0, max: 1000 };
        this._y_axis = { min: 0, max: 10 };

        {
            this._data = [];

            let x = 0;
            let y = 5;
            while (x < this._x_axis.max + 10) {
                this._data.push({ x, y });

                x += 1; //10 * Math.random();
                y += 2 * (Math.random() - 0.5);

                y = (0.9 * y) + (0.1 * 5);
            }
        }
    }

    componentDidMount() {
        let rect = this._root.getBoundingClientRect();

        this._ctx = this._canvas.getContext('2d');

        let canvas_height = 200;
        let canvas_width = rect.width;

        this.setState({
            canvas_height,
            canvas_width,
            graph_rect: {
                x: this._options.margin.left,
                width: canvas_width - (this._options.margin.right + this._options.margin.left),
                y: this._options.margin.top,
                height: canvas_height - (this._options.margin.top + this._options.margin.bottom)
            }
        }, () => {
            this._make_request();
        });
    }

    async _make_request() {
        let now = (new Date()).getTime() * 1000;
        let end_timestamp = now;
        let start_timestamp = end_timestamp - (60 * 60 * 1000000);

        const resp = await fetch('/api/query', {
            method: 'POST',
            cache: 'no-cache',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                start_timestamp,
                end_timestamp,
                metric_name: 'random'
            })
        });

        let obj = await resp.json();

        this._x_axis = { min: start_timestamp / 1000, max: end_timestamp / 1000 };

        this._data = [];
        for (var i = 0; i < obj.lines[0].points.length; i++) {
            let x = obj.lines[0].points[i].timestamp * 1;
            if (x < start_timestamp || x > end_timestamp) {
                console.error('Bad point');
            }

            this._data.push({ x: x / 1000, y: obj.lines[0].points[i].value });
        }

        // console.log('Num points: ' + this._data.length);

        // this._data = this._data.slice(0, 100);

        this._draw_frame();

        // TODO: Run this relative to the time at which we started to do the current refresh.
        setTimeout(() => this._make_request(), 2000);
    }

    _to_canvas_pt(pt: Point): Point {
        return {
            x: ((pt.x - this._x_axis.min) / (this._x_axis.max - this._x_axis.min)) * this.state.graph_rect.width + this.state.graph_rect.x,
            // TODO: Must invert this.
            y: ((pt.y - this._y_axis.min) / (this._y_axis.max - this._y_axis.min)) * this.state.graph_rect.height + this.state.graph_rect.y
        };
    }

    _draw_frame() {
        this._ctx.save();

        this._ctx.clearRect(0, 0, this.state.canvas_width, this.state.canvas_height);

        this._ctx.font = this._options.font.style;

        this._ctx.strokeStyle = '#ccc';
        this._ctx.lineWidth = 1;

        let candidate_intervals = [
            5 * 60 * 1000, // 5 minutes
            10 * 60 * 1000, // 10 minutes
            30 * 60 * 1000, // 30 minutes
            60 * 60 * 1000, // 1 hour
        ];

        let duration = this._x_axis.max - this._x_axis.min;

        let interval = candidate_intervals[0];
        for (let i = 0; i < candidate_intervals.length; i++) {
            if (duration / candidate_intervals[i] > 5) {
                interval = candidate_intervals[i];
            } else {
                break;
            }
        }

        let x_ticks = [];
        let current_tick = Math.floor(this._x_axis.min / interval) * interval;
        while (current_tick < this._x_axis.max) {
            let time = new Date(current_tick);
            let label = time.getHours().toString().padStart(2, '0') + ':' + time.getMinutes().toString().padStart(2, '0');

            x_ticks.push({ value: current_tick, label });
            current_tick += interval;
        }

        let y_ticks = [0, 2.5, 5, 7.5, 10];


        x_ticks.map((tick) => {
            if (tick.value < this._x_axis.min || tick.value > this._x_axis.max) {
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

            this._ctx.fillText(label, x_canvas - (dims.width / 2), y2 + this._options.font.size + 4);
        });

        y_ticks.map((tick) => {
            let y_canvas = this._to_canvas_pt({ x: NaN, y: tick }).y;
            y_canvas = Math.round(y_canvas + 0.5) - 0.5;

            this._ctx.beginPath();
            this._ctx.moveTo(this.state.graph_rect.x, y_canvas);
            this._ctx.lineTo(this.state.graph_rect.x + this.state.graph_rect.width, y_canvas);
            this._ctx.stroke();

            let label = tick + '';
            let dims = this._ctx.measureText(label);

            let h = dims.actualBoundingBoxAscent + dims.actualBoundingBoxDescent;

            this._ctx.fillText(label, this.state.graph_rect.x - dims.width - 10, y_canvas + h / 2);
        })

        this._ctx.strokeStyle = '#4af';
        this._ctx.fillStyle = '#4af';


        this._ctx.beginPath();
        this._ctx.rect(this.state.graph_rect.x, this.state.graph_rect.y, this.state.graph_rect.width, this.state.graph_rect.height);
        this._ctx.clip();

        this._ctx.beginPath();

        let closest_graph_pt = null;
        let closest_distance = 10; // Must be within 10 pixels to allow a match at all.

        let is_first = true;
        this._data.map((graph_pt) => {
            let pt = this._to_canvas_pt(graph_pt);

            if (is_first) {
                this._ctx.moveTo(pt.x, pt.y);
                is_first = false;
            } else {
                this._ctx.lineTo(pt.x, pt.y);
            }

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


        if (closest_graph_pt !== null) {
            let pt = this._to_canvas_pt(closest_graph_pt);

            this._ctx.beginPath();
            this._ctx.ellipse(pt.x, pt.y, 3, 3, 0, 0, 2 * Math.PI);
            this._ctx.fill();


            let position = {
                x: this._mouse_canvas_pos.x + 20,
                y: this._mouse_canvas_pos.y + 20
            }

            this.setState({
                tooltip: {
                    position,
                    right_align: false,

                    x_value: round_digits(closest_graph_pt.x, 2) + '',

                    lines: [
                        {
                            label: 'Sensor 1',
                            y_value: round_digits(closest_graph_pt.y, 2) + '',
                            color: '#4af'
                        }
                    ]
                }
            })
        } else if (this.state.tooltip) {
            this.setState({ tooltip: null })
        }

        this._ctx.restore();
    }

    _on_mouse_move(e: React.MouseEvent<HTMLDivElement, MouseEvent>) {
        let canvas_rect = this._canvas.getBoundingClientRect();
        this._mouse_canvas_pos = { x: e.clientX - canvas_rect.x, y: e.clientY - canvas_rect.y };


        // TODO: Debounce me.
        this._draw_frame();
    }

    _on_mouse_out() {
        this._mouse_canvas_pos = null;
        this._draw_frame();
    }

    render() {
        // TODO: Don't update the canvas as it will end up having dynamic width/height.
        return (
            <div style={{ fontSize: 0, position: 'relative' }} ref={(el) => { this._root = el; }} onMouseMove={(e) => this._on_mouse_move(e)} onMouseOut={() => this._on_mouse_out()}>
                <canvas width={this.state.canvas_width} height={this.state.canvas_height} style={{ cursor: 'pointer' }} ref={(el) => { this._canvas = el; }}></canvas>
                {this.state.tooltip ? <Tooltip data={this.state.tooltip} /> : null}
            </div>
        );
    }
};

class GraphCard extends React.Component {
    render() {
        return (
            <div className="card">
                <div className="card-header">
                    Temperature
                </div>
                <div className="card-body">
                    <Figure />
                </div>
            </div>
        );
    }

}

class NavBar extends React.Component {
    render() {
        return (
            <nav className="navbar navbar-dark bg-dark">
                <div className="container-fluid">
                    <a className="navbar-brand" href="#">Sensor Monitor</a>
                </div>
            </nav>
        );
    }
}

interface DropDownProps {
    inner_focus?: boolean,
    items: React.ReactElement[]
}

class DropDown extends React.Component<DropDownProps> {

    state = {
        open: false
    }

    _root_el: HTMLDivElement;

    _on_focus(e: React.FocusEvent<HTMLDivElement>) {
        this.setState({ open: true });
    }

    _on_blur(e: React.FocusEvent<HTMLDivElement>) {
        if (this.props.inner_focus) {
            setTimeout(() => {
                let el = document.activeElement;
                while (el !== null) {
                    if (el === this._root_el) {
                        return;
                    }

                    el = el.parentElement;
                }

                this.setState({ open: false });
            });
        } else {
            this.setState({ open: false });
        }
    }

    render() {
        return (
            <div ref={(el) => { this._root_el = el; }} onFocus={(e) => this._on_focus(e)} onBlur={(e) => this._on_blur(e)}
                style={{ position: 'relative', display: 'inline-block' }}>
                <button className="btn btn-sm btn-outline-secondary dropdown-toggle" role="button">
                    {this.props.children}
                </button>

                <ul tabIndex={0} className={"dropdown-menu" + (this.state.open ? ' show' : '')} style={{ right: 0 }}>
                    {this.props.items}
                </ul>
            </div>
        );
    }
}

/*
                    {this.props.items.map((item) => {
                        <li><a className="dropdown-item" href="#">Action</a></li>
                    })}

                    <li><a className="dropdown-item" href="#">Another action</a></li>
                    <li><a className="dropdown-item" href="#">Something else here</a></li>

*/


class App extends React.Component {
    render() {
        return (
            <div>
                <NavBar />
                <div className="container-fluid" style={{ paddingTop: '0.75em' }}>
                    <div style={{ paddingBottom: 10, textAlign: 'right' }}>
                        <DropDown items={[]}></DropDown>
                    </div>

                    <GraphCard />
                </div>
            </div>
        );
    }
};


let node = document.getElementById("app-root");
console.log("Place in", node);
ReactDOM.render(<App />, node)

console.log("Hello world");