import React from "react";
import { round_digits } from "pkg/web/lib/formatting";
import { PageContext } from "../page";
import { Button } from "pkg/web/lib/button";
import { EditInput } from "pkg/web/lib/input";
import { run_machine_command } from "../rpc_utils";
import { Card, CardBody } from "../card";
import { PropertiesTable } from "../properties_table";
import { extend, shallow_copy } from "pkg/web/lib/utils";
import { Point } from "pkg/web/lib/figure/types";
import { MachineUiState } from "./state";
import { clean_point } from "pkg/web/lib/figure/utils";

/*
Macros:
- To Anchor 1
- Set X0 Y0

Helper script:

- `M496.3` : Move to anchor 1
    - Base anchor

- `G54` : Use coordinate system #1
- `G10 L2 P1 X-360.158 Y-234.568 Z-3` : Set anchor 0 to be (0,0,0)
    - Offset X,Z

G10 L2 P1 X-360.1580 Y-234.5680 Z-3.000 X-360.1580 Y-234.5680 Z-3.000

- `M495 X15 Y15 C100 D60 O0 F0 A85 B45 I3 J3 H5 P1`
    - X,Y,width,height pulled from program
    - 

- As soon as we have a program loaded, we can render this data.

TODO: Need some status info to tell if auto leveling data is active.


TODO: Need to in some way block running the program if we haven't configured everything correct 
(e.g. with no origin set, the program will go out of bounds)

*/

export interface CarveraLevelingState {
    preview: boolean;
    origin_base: string;
    origin_offset: Point;
    scan_margin: boolean;
    z_probe: boolean;
    auto_level: boolean;
    auto_level_height: number;
    auto_level_grid: Point;
}

const DEFAULT_STATE: CarveraLevelingState = {
    preview: false,
    origin_base: 'anchor1',
    origin_offset: { x: 0, y: 0 },
    scan_margin: true,
    z_probe: true,
    auto_level: true,
    auto_level_height: 3,
    auto_level_grid: { x: 5, y: 5 }
};


// NOTE: All the points are in machine coordinates.
export interface CarveraLevelingRequest {
    origin: Point;
    probe_points: Point[];
    program_min: Point;
    program_max: Point;
    commands: string[];
}

export function resolve_leveling_state(machine: any, state: CarveraLevelingState): CarveraLevelingRequest | null {
    let origin: Point;
    if (state.origin_base == 'anchor1') {
        origin = { x: -360.158, y: -234.568 };
    } else if (state.origin_base == 'current_pos') {
        // TODO: Deduplicate this code with the PositionBox code.
        let x_pos = null;
        let y_pos = null;
        (machine.state.axis_values || []).map((axis) => {
            if (axis.id == "X") {
                x_pos = (axis.value || [null])[0];
            }
            if (axis.id == "Y") {
                y_pos = (axis.value || [null])[0];
            }
        });

        if (x_pos === null || y_pos === null) {
            return null;
        }

        origin = { x: x_pos, y: y_pos };
    } else {
        return null;
    }

    origin.x += state.origin_offset.x;
    origin.y += state.origin_offset.y;

    // TODO: Verify using the G54 one and there are no other ones.
    if (!machine.state.loaded_program?.file?.program?.bounds[0].min_position) {
        // TODO: Return an error message.
        return null;
    }

    let min_pos = clean_point(machine.state.loaded_program.file.program.bounds[0].min_position);
    let max_pos = clean_point(machine.state.loaded_program.file.program.bounds[0].max_position);

    min_pos.x += origin.x;
    min_pos.y += origin.y;
    max_pos.x += origin.x;
    max_pos.y += origin.y;

    let probe_points = [];
    if (state.auto_level) {
        let { x, y } = state.auto_level_grid;
        if (!x || x < 2 || !y || y < 2) {
            return null;
        }

        let x_int = (max_pos.x - min_pos.x) / (x - 1);
        let y_int = (max_pos.y - min_pos.y) / (y - 1);

        for (var i = 0; i < x; i++) {
            for (var j = 0; j < y; j++) {
                probe_points.push({
                    x: min_pos.x + i * x_int,
                    y: min_pos.y + j * y_int
                });
            }
        }
    }

    let probe_cmd = `M495 X${(min_pos.x - origin.x).toFixed(2)} Y${(min_pos.y - origin.y).toFixed(2)}`;
    if (state.scan_margin) {
        probe_cmd += ` C${(max_pos.x - origin.x).toFixed(2)} D${(max_pos.y - origin.y).toFixed(2)}`;
    }

    if (state.auto_level) {
        probe_cmd += ` A${(max_pos.x - min_pos.x).toFixed(2)} B${(max_pos.y - min_pos.y).toFixed(2)} I${state.auto_level_grid.x} J${state.auto_level_grid.y} H${state.auto_level_height.toFixed(2)}`;
    }

    if (state.z_probe) {
        probe_cmd += ` O0 F0`;
    }

    probe_cmd += ' P1';

    return {
        origin,
        probe_points,
        program_min: min_pos,
        program_max: max_pos,
        commands: [
            `G54`,
            `G10 L2 P1 X${origin.x.toFixed(4)} Y${origin.y.toFixed(4)}`,
            probe_cmd
        ]
    };
}


export class CarveraBox extends React.Component<{ machine: any, context: PageContext, ui_state: MachineUiState }> {
    _on_run = async (done) => {
        // TODO: Need to wait for ATC_STATE to become 0. It will be 6 during probing.

        let machine = this.props.machine;
        let state = this.props.ui_state.carvera_state();

        try {
            let resolved = resolve_leveling_state(machine, state);
            if (!resolved) {
                throw 'Invalid parameters';
            }

            for (var i = 0; i < resolved.commands.length; i++) {
                let res = await this.props.context.channel.call('cnc.Monitor', 'RunMachineCommand', {
                    machine_id: this.props.machine.id,
                    send_serial_command: resolved.commands[i]
                });

                if (!res.status.ok()) {
                    throw res.status.toString();
                }
            }

        } catch (e) {
            this.props.context.notifications.add({
                text: 'Leveling failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        done();
    }


    _change(diff) {
        let v = extend(this.props.ui_state.carvera_state(), diff);
        this.props.ui_state.set_carvera_state(v);
    }

    render() {

        let machine = this.props.machine;

        let state = this.props.ui_state.carvera_state();

        if (machine.config.firmware != 'CARVERA' || !machine.state.loaded_program) {
            if (state) {
                this.props.ui_state.set_carvera_state(null);
            }

            return null;
        }

        if (!state) {
            state = shallow_copy(DEFAULT_STATE);
            this.props.ui_state.set_carvera_state(state);
        }

        let properties = [
            {
                name: "Preview",
                value: (
                    <input type="checkbox" checked={state.preview}
                        onChange={(e) => this._change({ preview: e.target.checked })} />
                )
            },
            {
                name: 'Origin: Base',
                value: (
                    <select className="form-control" value={state.origin_base} onChange={(e) => this._change({ origin_base: e.target.value })}>
                        <option value="current_pos">Current Position</option>
                        <option value="anchor1">Anchor 1</option>
                    </select>
                )
            },
            {
                name: "Origin: Offset",
                value: (
                    <XYInput value={state.origin_offset} onChange={(v) => this._change({ origin_offset: v })} />
                )
            },
            {
                name: "Scan Margin",
                value: (
                    <input type="checkbox" checked={state.scan_margin}
                        onChange={(e) => this._change({ scan_margin: e.target.checked })}
                    />
                )
            },
            {
                name: "Z Probe",
                value: (
                    <input type="checkbox" checked={state.z_probe}
                        onChange={(e) => this._change({ z_probe: e.target.checked })} />
                )
            },
            {
                name: "Auto Level",
                value: (
                    <input type="checkbox" checked={state.auto_level}
                        onChange={(e) => this._change({ auto_level: e.target.checked })} />
                )
            },
            {
                name: "Auto Level: Height",
                value: (
                    <input type="number" className="form-control" value={state.auto_level_height}
                        onChange={(e) => this._change({ auto_level_height: e.target.value * 1 })} />
                )
            },
            {
                name: "Auto Level: Grid",
                value: (
                    <XYInput value={state.auto_level_grid} onChange={(v) => this._change({ auto_level_grid: v })} />
                )
            },
        ];

        let resolved = resolve_leveling_state(machine, state);

        return (
            <Card id="carvera-level" header="Carvera Leveling" style={{ marginBottom: 10 }}>
                <CardBody>
                    <PropertiesTable properties={properties} style={{ verticalAlign: 'baseline' }} />
                    {resolved ? (
                        <div style={{ backgroundColor: '#fafafa', border: '1px solid #ccc', marginBottom: 15 }}>
                            {resolved.commands.map((v, i) => {
                                return (
                                    <div key={i} style={{ borderTop: (i != 0 ? '1px solid #ccc' : null), padding: 5, fontFamily: "Noto Sans Mono", fontSize: '0.8em' }}>
                                        {v}
                                    </div>
                                );
                            })}

                        </div>
                    ) : null}


                    <Button disabled={!resolved} style={{ width: '100%' }} preset="primary" onClick={this._on_run}>Run</Button>
                </CardBody>
            </Card>
        );
    }
};

class XYInput extends React.Component<{ value: any, onChange: any }> {

    render() {
        let v = this.props.value;

        return (
            <div>
                <div style={{ width: '50%', display: 'inline-block', paddingRight: 3 }}>
                    <div className="input-group">
                        <div className="input-group-text">X</div>
                        <input type="number" className="form-control"
                            value={v.x}
                            onChange={(e) => {
                                let new_v = shallow_copy(v);
                                new_v.x = e.target.value * 1;
                                this.props.onChange(new_v);
                            }}
                        />
                    </div>
                </div>
                <div style={{ width: '50%', display: 'inline-block', paddingLeft: 3 }}>
                    <div className="input-group">
                        <div className="input-group-text">Y</div>
                        <input type="number" className="form-control"
                            value={v.y}
                            onChange={(e) => {
                                let new_v = shallow_copy(v);
                                new_v.y = e.target.value * 1;
                                this.props.onChange(new_v);
                            }}
                        />
                    </div>
                </div>
            </div>

        );
    }
}