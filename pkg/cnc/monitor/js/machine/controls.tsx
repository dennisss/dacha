import React from "react";
import { round_digits } from "pkg/web/lib/formatting";
import { PageContext } from "../page";
import { Button } from "pkg/web/lib/button";
import { EditInput } from "pkg/web/lib/input";
import { run_machine_command } from "../rpc_utils";
import { Card, CardBody } from "../card";
import { SpinnerInline } from "pkg/web/lib/spinner";
import { MachineUiState } from "./state";

export class ControlsComponent extends React.Component<{ machine: any, context: PageContext, ui_state: MachineUiState }> {

    render() {
        let machine = this.props.machine;

        if (machine.state.connection_state != 'CONNECTED') {
            return (
                <div style={{ padding: 10, color: '#ccc', textAlign: 'center' }}>
                    Machine not connected.
                </div>
            );
        }

        return (
            <div>
                <JogControlsBox machine={machine} context={this.props.context} ui_state={this.props.ui_state} />
                <TemperaturesBox machine={machine} context={this.props.context} />
                <SensorsBox machine={machine} context={this.props.context} />
                <SwitchesBox machine={machine} context={this.props.context} />
            </div>
        );
    }

}

class JogControlsBox extends React.Component<{ machine: any, context: PageContext, ui_state: MachineUiState }> {

    _run_command = (command, done) => {
        run_machine_command(this.props.context, this.props.machine, command, done);
    }

    _make_buttons() {

        let machine = this.props.machine;

        if (machine.config.firmware == 'CARVERA') {
            return (
                <>
                    <Button onClick={(done) => this._run_command({ send_serial_command: 'M496.1' }, done)}
                        preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Clearance</Button>
                    <Button onClick={(done) => this._run_command({ send_serial_command: 'M496.3' }, done)}
                        preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Anchor 1</Button>
                    <CarveraPairWPButton {...this.props} />
                </>
            );
        }

        return (
            <>
                <Button onClick={(done) => this._run_command({ home_x: true }, done)}
                    preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Home X</Button>
                <Button onClick={(done) => this._run_command({ home_y: true }, done)}
                    preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Home Y</Button>
                <Button onClick={(done) => this._run_command({ home_all: true }, done)}
                    preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Home All</Button>
                <Button onClick={(done) => this._run_command({ mesh_level: true }, done)}
                    preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Mesh Level</Button>
            </>
        );
    }

    render() {
        let machine = this.props.machine;

        let axis_values = {};
        machine.state.axis_values.map((v) => {
            axis_values[v.id] = v.value;
        })

        // TODO: Also need a configurable job feed rate.

        let work_coordinates = null;

        if (machine.state.coordinate_systems) {
            machine.state.coordinate_systems.map((c) => {
                if (!c.current) {
                    return;
                }

                work_coordinates = c;
            });
        }

        return (
            <Card id="jog" header={
                <>
                    Jog

                    {machine.state.firmware_state ? (
                        // TODO: Color this based on the state.
                        <div style={{ float: 'right' }}>
                            <span className={"badge rounded-pill bg-secondary"}>
                                {machine.state.firmware_state}
                            </span>
                        </div>

                    ) : null}
                </>
            } style={{ marginBottom: 10 }}>
                <CardBody>
                    <div style={{ display: 'flex' }}>
                        <JogButtons machine={machine} context={this.props.context} ui_state={this.props.ui_state} />
                        <div style={{ paddingTop: 3 }}>
                            <Button onClick={(done) => this._run_command({ full_stop: true }, done)}
                                preset="danger" style={{ width: '100%', marginBottom: 5 }}>Stop!</Button>
                            {this._make_buttons()}
                        </div>
                    </div>

                    <RateInputs {...this.props} />

                    <div style={{ paddingTop: 10 }}>
                        <table className="table" style={{ verticalAlign: 'baseline', margin: 0 }}>
                            <thead>
                                <tr>
                                    <th>Axis</th>
                                    <th style={{ textAlign: 'right' }}>Machine Position</th>
                                    <th style={{ textAlign: 'right' }}>Work Position{work_coordinates ? ` (${work_coordinates.gcode})` : ''}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {machine.config.axes.map((axis) => {
                                    if (axis.type != 'POSITION') {
                                        return null;
                                    }

                                    let machine_pos = axis_values[axis.id][0];
                                    let work_pos = undefined;

                                    if (work_coordinates) {
                                        (work_coordinates.offset || []).map((offset) => {
                                            if (offset.id == axis.id) {
                                                work_pos = machine_pos - offset.value[0];
                                            }
                                        })
                                    }

                                    // TODO: Render all the numbers at fixed precision and with right alignment.
                                    return (
                                        <tr key={axis.id}>
                                            <td>{axis.name || axis.id}</td>
                                            <td style={{ fontFamily: 'Noto Sans Mono', textAlign: 'right' }}>
                                                {format_float(machine_pos)}
                                            </td>
                                            <td style={{ fontFamily: 'Noto Sans Mono', textAlign: 'right' }}>
                                                {work_pos === undefined ? 'N/A' : format_float(work_pos)}
                                            </td>
                                        </tr>
                                    );

                                })}
                                <ToolSelector machine={machine} context={this.props.context} />
                            </tbody>
                        </table>
                    </div>
                </CardBody>
            </Card>
        );
    }
};


class CarveraPairWPButton extends React.Component<{ machine: any, context: PageContext }> {

    state = {
        _pairing_end_time: 0
    }

    _run_command = (command, done) => {
        run_machine_command(this.props.context, this.props.machine, command, done);
    }

    _click = (done) => {
        this._run_command({ send_serial_command: 'M471' }, () => {
            done();

            let now = new Date();
            let end_time = now.getTime() + 30 * 1000;

            this.setState({ _pairing_end_time: end_time });

            let interval = setInterval(() => {
                if (new Date().getTime() > this.state._pairing_end_time) {
                    clearInterval(interval);
                }
                this.forceUpdate();
            }, 500);
        });
    }

    render() {
        let now = new Date().getTime();
        let end_time = this.state._pairing_end_time || now;

        let remaining_time = (end_time - now) / 1000;
        let still_pairing = remaining_time > 0;

        return (
            <Button disabled={still_pairing} onClick={this._click}
                preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>{still_pairing ? (Math.round(remaining_time) + 's') : 'Pair WP'}</Button>
        );
    }
}

class JogButtons extends React.Component<{ machine: any, context: PageContext, ui_state: MachineUiState }> {

    state = {
        _increment: 1,
    }

    _on_click_arrow = async (axis_id: string, direction: number) => {

        // TODO: Limit how quickly the user can press these buttons.

        let ctx = this.props.context;

        try {
            let res = await ctx.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: this.props.machine.id,
                jog: {
                    feed_rate: this.props.ui_state.jog_feed_rate(),
                    increment: [{
                        axis_id: axis_id,
                        value: this.state._increment * direction
                    }]
                }
            });
            if (!res.status.ok()) {
                throw res.status.toString();
            }
        } catch (e) {
            console.error(e);
            // TODO: Notification
        }


    }

    render() {
        // TODO: Need to split up feedrate between axes and use configured defaults.

        let increments = [
            0.1, 1, 10, 100
        ];

        return (
            <div>
                <div>
                    <table>
                        <tbody>
                            <tr>
                                <td></td>
                                <td>
                                    <JogButton rotate={-90} onClick={() => this._on_click_arrow('Y', 1)} />
                                </td>
                                <td></td>
                                <td><div style={{ width: '1em' }}></div></td>
                                <td>
                                    <JogButton rotate={-90} onClick={() => this._on_click_arrow('Z', 1)} />
                                </td>
                            </tr>
                            <tr>
                                <td>
                                    <JogButton rotate={180} onClick={() => this._on_click_arrow('X', -1)} />
                                </td>
                                <td style={{ textAlign: 'center' }}>X/Y</td>
                                <td>
                                    <JogButton rotate={0} onClick={() => this._on_click_arrow('X', 1)} />
                                </td>
                                <td></td>
                                <td style={{ textAlign: 'center' }}>Z</td>
                            </tr>
                            <tr>
                                <td></td>
                                <td>
                                    <JogButton rotate={90} onClick={() => this._on_click_arrow('Y', -1)} />
                                </td>
                                <td></td>
                                <td></td>
                                <td>
                                    <JogButton rotate={90} onClick={() => this._on_click_arrow('Z', -1)} />
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>

                <div style={{ paddingTop: 10 }}>
                    <div style={{ fontSize: '0.8em' }}>
                        Increment:
                    </div>
                    <div className="btn-toolbar mb-3">
                        <div className="btn-group me-2" role="group">
                            {increments.map((amount, i) => {
                                let active = this.state._increment == amount;

                                return (
                                    <button key={i} onClick={() => this.setState({ _increment: amount })} type="button" className={"btn " + (active ? 'btn-outline-dark active' : 'btn-outline-secondary')}>{amount}mm</button>
                                );
                            })}
                        </div>
                    </div>
                </div>
            </div>
        );
    }
}

class RateInputs extends React.Component<{ machine: any, ui_state: MachineUiState, context: PageContext }> {
    render() {

        let { machine, ui_state } = this.props;

        return (
            <div>
                {/* TODO: Consider syncing this with the current feedrate being used by the machine (rather than it just being the manual job rate) */}
                <div style={{ width: 130, display: 'inline-block', marginRight: 10 }}>
                    <div style={{ fontSize: '0.8em' }}>
                        Feedrate (mm/min):
                    </div>
                    <input type="text" className="form-control" value={ui_state.jog_feed_rate() + ''}
                        onChange={(e) => ui_state.set_job_feed_rate(e.target.value * 1)} />
                </div>

                {machine.config.spindle ? (
                    <SpindleInput {...this.props} />
                ) : null}
            </div>
        )
    }
}

class SpindleInput extends React.Component<{ machine: any, ui_state: MachineUiState, context: PageContext }> {
    state = {
        _editing: false
    }

    _on_set_spindle = async (value, done) => {
        let v = value * 1;
        if (Number.isNaN(v)) {
            done();
            return;
        }

        let ctx = this.props.context;
        try {
            let res = await ctx.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: this.props.machine.id,
                set_spindle_state: {
                    mode: v > 0 ? 'ON_CLOCKWISE' : 'OFF',
                    target_speed_rpm: v
                }
            });
            if (!res.status.ok()) {
                throw res.status.toString();
            }
        } catch (e) {
            console.error(e);
            // TODO: Notification
        }

        done();
    }


    render() {
        let { machine, ui_state } = this.props;

        let on = machine.state.spindle.mode.startsWith('ON_');
        let target_rate = on ? (machine.state.spindle.target_speed_rpm || 0) : 0

        return (
            <div style={{ display: 'inline-block', width: 270 }}>
                <div style={{ fontSize: '0.8em' }}>
                    Spindle (RPM):
                </div>

                <div style={{ fontFamily: 'Noto Sans Mono', textAlign: 'right' }}>
                    {machine.config.spindle.supports_current_speed && !this.state._editing ? (
                        <div style={{ width: 130, display: 'inline-block' }}>
                            {format_float(machine.state.spindle.current_speed_rpm)}&nbsp;/&nbsp;
                        </div>
                    ) : null}
                    <div style={{ width: (this.state._editing ? 200 : 140), display: 'inline-block' }}>
                        <EditInput value={target_rate + ''} onChange={this._on_set_spindle} onActive={(v) => this.setState({ _editing: v })} />
                    </div>
                </div>
            </div>
        );

    }
}

class JogButton extends React.Component<{ onClick: any, rotate: number }> {

    render() {
        return (
            <button className="btn btn-outline-dark" style={{ width: 60, height: 60, border: '1px solid #000', borderRadius: 5, position: 'relative', margin: 2 }} onClick={this.props.onClick}>
                <div style={{ position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', fontWeight: 'bold', fontSize: '1.5em' }}>

                    <div style={{ transform: "rotate(" + this.props.rotate + "deg)" }}>
                        <span className="material-symbols-outlined">
                            chevron_right
                        </span>
                    </div>
                </div>
            </button>
        );
    }

}

class ToolSelector extends React.Component<{ machine: any, context: PageContext }> {

    state = {
        _pending_index: null
    }

    _on_change = async (e) => {
        let machine = this.props.machine;
        let new_index = e.target.value * 1;
        let active_tool = machine.state.tools.active_tool || 0;

        if (new_index == active_tool) {
            return;
        }

        // Disallow two concurrent tool swaps.
        if (this.state._pending_index !== null) {
            return;
        }

        this.setState({ _pending_index: new_index });

        try {
            // TODO: Need this to have a timeout.
            let res = await this.props.context.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: this.props.machine.id,
                tool_change: new_index
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }
        } catch (e) {
            this.props.context.notifications.add({
                text: 'Tool change failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        this.setState({ _pending_index: null });
    }

    render() {
        let machine = this.props.machine;
        if (!machine.config.tools || !machine.config.tools.num_slots) {
            return null;
        }

        let active_tool = machine.state.tools.active_tool || 0;

        let selected_tool = this.state._pending_index !== null ? this.state._pending_index : active_tool;
        let switching = selected_tool !== active_tool;

        return (
            <tr>
                <td>Tool</td>
                <td colSpan={2}>
                    <div className="input-group">
                        <select className="form-select" value={selected_tool} disabled={switching} onChange={this._on_change}>
                            <option value={-1}>None</option>
                            {(machine.config.tools.loaded_tools || []).map((tool) => {
                                let index = tool.index || 0;
                                return (
                                    <option key={index} value={index}>
                                        ({index}) {tool.name}
                                    </option>
                                );
                            })}
                        </select>

                        {switching ? (
                            <span className="input-group-text"><SpinnerInline /></span>
                        ) : null}
                    </div>

                </td>
            </tr>
        )
    }
}

class TemperaturesBox extends React.Component<{ machine: any, context: PageContext }> {

    _on_set_temperature = async (axis_id, value, done) => {
        try {
            // TODO: Need this to have a timeout.
            let res = await this.props.context.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: this.props.machine.id,
                set_temperature: {
                    axis_id: axis_id,
                    target: value
                }
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }

        } catch (e) {
            this.props.context.notifications.add({
                text: 'Send failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        done();
    }

    render() {
        let machine = this.props.machine;

        let axis_values = {};
        machine.state.axis_values.map((v) => {
            axis_values[v.id] = v.value;
        })

        let rows = [];
        machine.config.axes.map((axis) => {
            if (axis.type != 'HEATER' || axis.hide) {
                return;
            }

            rows.push(
                <tr key={axis.id}>
                    <td>{axis.name || axis.id}</td>
                    <td>{round_digits(axis_values[axis.id][0], 2)}</td>
                    <td>
                        <EditInput value={round_digits(axis_values[axis.id][1], 2) + ''}
                            onChange={(v, done) => {
                                this._on_set_temperature(axis.id, v, done);
                            }} />
                    </td>
                </tr>
            );
        });

        if (rows.length == 0) {
            return null;
        }

        return (
            <Card id="temps" header="Temperatures" style={{ marginBottom: 10 }}>
                <CardBody>
                    <div>
                        <table className="table" style={{ verticalAlign: 'baseline', margin: 0 }}>
                            <thead>
                                <tr>
                                    <th>Heater</th>
                                    <th>Current (C)</th>
                                    <th style={{ width: 180 }}>Target (C)</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows}
                            </tbody>
                        </table>
                    </div>
                </CardBody>
            </Card>
        );
    }
};

function format_float(v) {
    return (v || 0).toFixed(2);
}

class SensorsBox extends React.Component<{ machine: any, context: PageContext }> {
    render() {
        let machine = this.props.machine;

        let axis_values = {};
        machine.state.axis_values.map((v) => {
            axis_values[v.id] = v.value;
        })

        let rows = [];
        machine.config.axes.map((axis) => {
            if (axis.type != 'GENERIC_SENSOR' || axis.hide) {
                return;
            }

            // TODO: THis sometimes gets an out of bounds for the '[0]' part so for some reason we don't have an 'axis_values' entry.
            rows.push(
                <tr key={axis.id}>
                    <td>{axis.name || axis.id}</td>
                    <td style={{ textAlign: 'right', }}>
                        {format_float(axis_values[axis.id][0])}
                    </td>
                </tr>
            );
        });

        if (rows.length == 0) {
            return null;
        }

        return (
            <Card id="sensors" header="Sensors" style={{ marginBottom: 10, fontFamily: 'Noto Sans Mono' }}>
                <CardBody>
                    <div>
                        <table className="table" style={{ verticalAlign: 'baseline', margin: 0 }}>
                            <tbody>
                                {rows}
                            </tbody>
                        </table>
                    </div>
                </CardBody>
            </Card>
        );
    }
};


class SwitchesBox extends React.Component<{ machine: any, context: PageContext }> {

    // TODO: Also implement changing the value of PWM switches.
    async _toggle_switch(axis, value) {
        try {

            let cmd = value ? axis.switch.on : axis.switch.off;

            // TODO: Need this to have a timeout.
            let res = await this.props.context.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: this.props.machine.id,
                send_serial_command: cmd
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }


        } catch (e) {
            this.props.context.notifications.add({
                text: 'Send failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }
    }

    render() {
        let machine = this.props.machine;

        let axis_values = {};
        machine.state.axis_values.map((v) => {
            axis_values[v.id] = v.value;
        })

        let rows = [];
        machine.config.axes.map((axis) => {
            if (axis.type != 'SWITCH' || axis.hide) {
                return;
            }

            let can_change = axis.switch && axis.switch.on;

            rows.push(
                <tr key={axis.id}>
                    <td>{axis.name || axis.id}</td>
                    <td>
                        {axis_values[axis.id].length >= 2 ? format_float(axis_values[axis.id][1]) : ''}
                    </td>
                    <td>
                        <input type="checkbox" disabled={!can_change} checked={axis_values[axis.id][0] != 0}
                            onChange={(e) => this._toggle_switch(axis, e.target.checked)}

                        />
                    </td>

                </tr>
            );
        });

        if (rows.length == 0) {
            return null;
        }

        return (
            <Card id="switches" header="Switches" style={{ marginBottom: 10 }}>
                <CardBody>
                    <div>
                        <table className="table" style={{ verticalAlign: 'baseline', margin: 0 }}>
                            <thead>
                                <tr>
                                    <th>Switch</th>
                                    <th>Value</th>
                                    <th>On</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows}
                            </tbody>
                        </table>
                    </div>
                </CardBody>
            </Card>
        );
    }
};
