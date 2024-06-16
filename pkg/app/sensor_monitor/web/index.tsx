import React from 'react';
import ReactDOM from 'react-dom';

import { Channel } from "pkg/web/lib/rpc";
import { Figure } from 'pkg/web/lib/figure';
import { EntityKind, FigureOptions } from 'pkg/web/lib/figure/types';


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

function get_test_options(): FigureOptions {

    let x_range = { min: 0, max: 1000 };
    let y_range = { min: 0, max: 10 };

    // Make some fake data to display.
    let data = [];
    {
        let x = 0;
        let y = 5;
        while (x < x_range.max + 10) {
            data.push({ x, y });

            x += 1; //10 * Math.random();
            y += 2 * (Math.random() - 0.5);

            y = (0.9 * y) + (0.1 * 5);
        }
    }

    let x_ticks = [];
    {
        let candidate_intervals = [
            5 * 60 * 1000, // 5 minutes
            10 * 60 * 1000, // 10 minutes
            30 * 60 * 1000, // 30 minutes
            60 * 60 * 1000, // 1 hour
        ];

        let duration = x_range.max - x_range.min;

        let interval = candidate_intervals[0];
        for (let i = 0; i < candidate_intervals.length; i++) {
            if (duration / candidate_intervals[i] > 5) {
                interval = candidate_intervals[i];
            } else {
                break;
            }
        }

        let current_tick = Math.floor(x_range.min / interval) * interval;
        while (current_tick < x_range.max) {
            let time = new Date(current_tick);
            let label = time.getHours().toString().padStart(2, '0') + ':' + time.getMinutes().toString().padStart(2, '0');

            x_ticks.push({ value: current_tick, label });
            current_tick += interval;
        }
    }

    return {
        width: '100%',
        height: 200,

        margin: {
            left: 40,
            bottom: 20,
            top: 6,
            right: 2
        },
        font: {
            style: '14px "Noto Sans"',
            size: 14
        },

        x_axis: {
            range: x_range,
            ticks: x_ticks
        },

        y_axis: {
            range: y_range,
            ticks: [0, 2.5, 5, 7.5, 10].map((v) => {
                return {
                    value: v,
                    label: v + ''
                };
            })
        },

        entities: [
            {
                kind: EntityKind.LineGraph,
                label: 'Sensor 1',
                color: '#4af',
                data: data
            }
        ]
    };
}



class GraphCard extends React.Component<{}, { _options: FigureOptions }> {

    state = {
        _options: get_test_options()
    }

    componentDidMount(): void {
        this._make_request();
    }

    async _make_request() {
        let now = (new Date()).getTime() * 1000;
        let end_timestamp = now;
        let start_timestamp = end_timestamp - (60 * 60 * 1000000);

        let channel = new Channel("http://localhost:8001");

        let res = await channel.call('Metric', 'Query', {
            start_timestamp,
            end_timestamp,
            metric_name: 'random'
        });

        let obj = res.responses[0];

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

        // TODO: Update the state.

        // TODO: Run this relative to the time at which we started to do the current refresh.
        setTimeout(() => this._make_request(), 2000);
    }

    render() {
        return (
            <div className="card">
                <div className="card-header">
                    Temperature
                </div>
                <div className="card-body">
                    <Figure options={this.state._options} />
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