import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { PageContext } from "../page";
import { Button } from "pkg/web/lib/button";
import { LabeledCheckbox } from "pkg/web/lib/checkbox";
import { Card, CardBody } from "../card";
import { Figure } from "pkg/web/lib/figure";
import { EntityKind, FigureOptions } from "pkg/web/lib/figure/types";
import { deep_copy, shallow_copy } from "pkg/web/lib/utils";
import { round_digits } from "pkg/web/lib/formatting";



const COLORS = [
    '#4af',
    '#158',
    '#4e5',
    '#147d22',
];

export interface MetricsBoxProps {
    context: PageContext;
    machine: any

    // This are mainly set when seeking to a historical run.
    // TODO: Need to implement support for reacting to changes in these props.
    startTime?: Date;
    endTime?: Date;
}

interface MetricsBoxState {
    _data: MetricsData
}

// TODO: Also need date formatting since may be on past days.


interface MetricsData {
    start_time: number;

    end_time: number;

    // Time in seconds at which end_time will be advanced with new data.
    // If 0, then we are viewing a static historical window of time that won't be refreshed.
    refresh_interval: number;

    max_interpolation_gap: number;

    entries: SeriesEntry[]
}

interface SeriesEntry {
    name: string;
    color: string;
    data: any[]
    line_width: number
}

function get_test_options(data: MetricsData): FigureOptions {

    // TODO: Gray out any ranges of data that aren't loaded yet.

    // X range is in milliseconds.

    let x_range = { min: data.start_time, max: data.end_time };
    let y_range = { min: 0, max: 350 };

    /*
    // Make some fake data to display.
    let data = [];
    {
        let x = x_range.min;
        let y = 5;
        while (x < x_range.max + 10) {
            data.push({ x, y });

            x += (x_range.max - x_range.min) / 100; //10 * Math.random();
            y += 2 * (Math.random() - 0.5);

            y = (0.9 * y) + (0.1 * 5);
        }
    }
    */

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


    let y_values = [];
    for (var i = 0; i <= y_range.max; i += 50) {
        y_values.push(i);
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
            ticks: x_ticks,
            renderer: (v) => {
                // TODO: Dedup with above.
                let time = new Date(v);
                let label = time.getHours().toString().padStart(2, '0') + ':' + time.getMinutes().toString().padStart(2, '0');
                return label;
            }
        },

        y_axis: {
            range: y_range,
            ticks: y_values.map((v) => {
                return {
                    value: v,
                    label: v + ''
                };
            }),
            renderer: (v) => round_digits(v, 2) + ''
        },

        // TODO: Sort these by line width
        entities: data.entries.map((entry) => {
            return {
                kind: EntityKind.LineGraph,
                label: entry.name,
                color: entry.color,
                data: entry.data,
                width: entry.line_width,
                max_interpolation_gap: data.max_interpolation_gap
            };
        })
    };
}


export class MetricsBox extends React.Component<MetricsBoxProps, MetricsBoxState> {

    _abort_controller: AbortController = new AbortController();

    _data = {};

    constructor(props: MetricsBoxProps) {
        super(props);

        this._read_metrics();
    }

    componentWillUnmount(): void {
        this._abort_controller.abort();
    }

    // TODO: Filter updates based on only self state updates.

    // TODO: Need infinite retrying.
    // TODO: Need cancellation on component unmount.
    _read_metrics() {

        let machine = this.props.machine;

        let start_time;
        let end_time;
        let bounded_end_time;

        if (this.props.endTime) {
            end_time = this.props.endTime.getTime();
            bounded_end_time = true;
        } else {
            end_time = new Date().getTime();
            bounded_end_time = false;
        }

        if (this.props.startTime) {
            start_time = this.props.startTime.getTime();
        } else {
            start_time = end_time - 30 * 60 * 1000;  // 30 minutes
        }

        let alignment = (end_time - start_time) / 200;

        let data: MetricsData = {
            entries: [],
            start_time,
            end_time,
            max_interpolation_gap: alignment * 2,
            refresh_interval: 0
        };

        let resources = [];

        machine.config.axes.map((axis) => {
            if (!axis.collect || axis.type != 'HEATER' || axis.hide) {
                return;
            }


            for (var i = 0; i < 2; i++) {
                let resource = {
                    machine_id: machine.id,
                    kind: 'MACHINE_AXIS_VALUE',
                    axis_id: axis.id,
                    value_index: i
                };
                resources.push(resource);

                let name = axis.name + (i == 0 ? ' (Current)' : ' (Target)');
                let entry_index = data.entries.length;
                data.entries.push({
                    name,
                    color: COLORS[entry_index % COLORS.length],
                    data: [],
                    line_width: 1
                });
            }
        });

        this.state = {
            _data: data
        };

        this._read_metric(resources, start_time, bounded_end_time ? end_time : null, alignment)
    }

    async _read_metric(resources: any[], start_time: number, end_time: number | null, alignment: number) {
        // TODO: If we have already read some amount of data , re-use it.

        let res = this.props.context.channel.call_streaming('cnc.Monitor', 'QueryMetric', {
            resource: resources,
            start_time: start_time * 1000,
            end_time: (end_time ? end_time * 1000 : null),
            // TODO: Re-enable once improving this more.
            // alignment: alignment * 1000
        }, { abort_signal: this._abort_controller.signal });

        let is_first = true;
        while (true) {
            let msg = await res.recv();
            if (!msg) {
                // TODO: This is an error if end_time is not set
                return;
            }

            // TODO: Make this more efficient?
            let data = deep_copy(this.state._data);
            let window_duration = data.end_time - data.start_time;

            // Maximum amount (in milliseconds) of points to keep before start_time.
            let max_history = 2 * window_duration;

            data.end_time = msg.end_time / 1000;
            data.start_time = data.end_time - window_duration;

            (msg.streams || []).map((stream, stream_i) => {

                let new_samples = (stream.samples || []).map((sample) => {
                    let x = sample.timestamp / 1000; // micro to milliseconds.
                    let y = sample.float_value || 0.0;
                    return { x, y }
                });

                // TODO: This should simply require a reverse operation.
                // Or just do this operation on the server.
                new_samples.sort((a, b) => {
                    return a.x - b.x;
                });

                // TODO: If we exceed the interpolation gap between 3 adjacent points, we still want to render the middle point with a line rather than it just showing up as a point (as we only use moveTo). Conceptually we can think of it as a line of the 'alignment_size length' which 'ends' at the point time. 
                let combined_data = (is_first ? [] : data.entries[stream_i].data).concat(new_samples);

                // Truncate any very old data.
                var i = 0;
                for (; i < combined_data.length; i++) {
                    if (combined_data[i].x >= data.start_time - max_history) {
                        break;
                    }
                }
                combined_data.splice(0, i);

                data.entries[stream_i].data = combined_data;
            });

            is_first = false;

            this.setState({ _data: data });
        }

    }

    render() {
        if (this.state._data.entries.length == 0) {
            return null;
        }

        // TODO: Need a loading spinner.

        return (
            <Card id="metrics" header="Metrics">
                <CardBody>
                    <Figure options={get_test_options(this.state._data)} />

                    <div style={{ paddingTop: 10, fontSize: '0.8em', textAlign: 'center' }}>

                        {this.state._data.entries.map((entry, i) => {

                            return (
                                <SeriesButton key={i} entry={entry} setWidth={(w) => {
                                    let data = deep_copy(this.state._data);
                                    data.entries[i].line_width = w;
                                    this.setState({ _data: data });
                                }} />
                            );
                        })}
                    </div>

                </CardBody>
            </Card>
        );
    }
}

class SeriesButton extends React.Component<{ entry: SeriesEntry, setWidth: (w: number) => {} }> {

    state = {
        _hovering: false
    }

    _mouse_enter = () => {
        // The timeout is to ensure that the mouse_exit from another button gets applied before this one is applied.
        // TODO: The better way to do this is to verify at most one button has width >1.
        setTimeout(() => {
            if (this.props.entry.line_width >= 1) {
                this.props.setWidth(2);
            }
        }, 2);
    }

    _mouse_exit = () => {
        if (this.props.entry.line_width >= 1) {
            this.props.setWidth(1);
        }
    }

    _on_click = () => {
        let entry = this.props.entry;
        let on = entry.line_width >= 1;

        if (on) {
            this.props.setWidth(0);
        } else {
            this.props.setWidth(1);
        }

    }

    render() {
        let entry = this.props.entry;
        let on = entry.line_width >= 1;

        return (
            <div className="figure-series-button" onClick={this._on_click} onMouseEnter={this._mouse_enter} onMouseLeave={this._mouse_exit}>
                <div style={{ border: ('1px solid ' + entry.color), display: 'inline-block', marginRight: '1ex', width: 20, height: 10, backgroundColor: (on ? entry.color : null) }}></div>

                {entry.name}
            </div>
        );

    }

}
