import React from "react";
import { Figure } from "pkg/web/lib/figure";
import { EntityKind, FigureOptions, Point, Range } from "pkg/web/lib/figure/types";
import { PageContext } from "../page";
import { run_machine_command } from "../rpc_utils";


function clean_range(v: any): Range {
    return {
        min: v.min || 0,
        max: v.max || 0
    };
}


export class PositionBox extends React.Component<{ machine: any, context: PageContext }> {

    _get_figure_options(): FigureOptions {

        let machine = this.props.machine;

        let x_range = null;
        let y_range = null;
        machine.config.axes.map((axis) => {
            if (axis.id == 'X') {
                x_range = clean_range(axis.range);
            }
            if (axis.id == 'Y') {
                y_range = clean_range(axis.range);
            }
        });

        let entities = [];

        let work_x = clean_range(machine.config.work_area.x_range);
        let work_y = clean_range(machine.config.work_area.y_range);

        // NOTE: Here we sort of assume that the work_area min is at (0,0)
        // TODO: Have some indicator of whether or not the work_x|y.max ends exactly at a 10mm interval.
        {

            let x = 0;
            while (x < work_x.max) {
                entities.push({
                    kind: EntityKind.Line,
                    color: '#aaa',
                    width: (x % 50 == 0 ? 2 : 1),
                    start: { x: x, y: work_y.min },
                    end: { x: x, y: work_y.max }
                });

                x += 10;
            }

            let y = 0;
            while (y < work_y.max) {
                entities.push({
                    kind: EntityKind.Line,
                    color: '#aaa',
                    width: (y % 50 == 0 ? 2 : 1),
                    start: { x: work_x.min, y: y },
                    end: { x: work_x.max, y: y }
                });

                y += 10;
            }
        }


        // Border around the whole work area.
        // TODO: Make this into one continous and closed path
        {
            entities.push({
                kind: EntityKind.Line,
                color: '#444',
                width: 3,
                start: { x: work_x.min, y: work_y.min },
                end: { x: work_x.min, y: work_y.max }
            });
            entities.push({
                kind: EntityKind.Line,
                color: '#444',
                width: 3,
                start: { x: work_x.max, y: work_y.min },
                end: { x: work_x.max, y: work_y.max }
            });
            entities.push({
                kind: EntityKind.Line,
                color: '#444',
                width: 3,
                start: { x: work_x.min, y: work_y.min },
                end: { x: work_x.max, y: work_y.min }
            });
            entities.push({
                kind: EntityKind.Line,
                color: '#444',
                width: 3,
                start: { x: work_x.min, y: work_y.max },
                end: { x: work_x.max, y: work_y.max }
            });
        }


        let x_pos = -100;
        let y_pos = -100;
        (machine.state.axis_values || []).map((axis) => {
            if (axis.id == "X") {
                x_pos = axis.value[0];
            }
            if (axis.id == "Y") {
                y_pos = axis.value[0];
            }
        })

        entities.push({
            kind: EntityKind.Circle,
            center: { x: x_pos, y: y_pos },
            color: 'red',
            radius: 5
        });


        return {
            width: '100%',
            aspect_ratio: 1,

            margin: {
                left: 10,
                bottom: 10,
                top: 10,
                right: 10
            },
            font: {
                style: '14px "Noto Sans"',
                size: 14
            },

            x_axis: {
                range: x_range,
                ticks: []
            },

            y_axis: {
                range: y_range,
                ticks: []
            },

            entities: entities
        };


    }

    _on_click = (pt: Point) => {
        let ctx = this.props.context;

        run_machine_command(ctx, this.props.machine, {
            goto: {
                // TODO: Pull the feed rate from the other ui input.
                feed_rate: 1000,
                x: pt.x,
                y: pt.y,
            }

        }, () => { });
    }

    render() {
        return (
            <div className="card">
                <div className="card-header">
                    Top-down View
                </div>
                <div className="card-body">
                    <Figure options={this._get_figure_options()} onClick={this._on_click} />
                </div>
            </div>
        );
    }
};
