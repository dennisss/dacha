import React from "react";
import { round_digits } from "pkg/web/lib/formatting";
import { PageContext } from "../page";
import { Button } from "pkg/web/lib/button";
import { EditInput } from "pkg/web/lib/input";
import { run_machine_command } from "../rpc_utils";
import { Card, CardBody } from "../card";

export class ControlsComponent extends React.Component<{ machine: any, context: PageContext }> {

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
                <JogControlsBox machine={machine} context={this.props.context} />
                <TemperaturesBox machine={machine} context={this.props.context} />
            </div>
        );
    }

}

class JogControlsBox extends React.Component<{ machine: any, context: PageContext }> {

    _run_command = (command, done) => {
        run_machine_command(this.props.context, this.props.machine, command, done);
    }

    render() {
        let machine = this.props.machine;

        let axis_values = {};
        machine.state.axis_values.map((v) => {
            axis_values[v.id] = v.value;
        })

        // TODO: Also need a configurable job feed rate.

        return (
            <Card id="jog" header="Jog" style={{ marginBottom: 10 }}>
                <CardBody>
                    <div style={{ display: 'flex' }}>
                        <JogButtons machine={machine} context={this.props.context} />
                        <div style={{ paddingTop: 3 }}>
                            <Button onClick={(done) => this._run_command({ full_stop: true }, done)}
                                preset="danger" style={{ width: '100%', marginBottom: 5 }}>Stop!</Button>
                            <Button onClick={(done) => this._run_command({ home_x: true }, done)}
                                preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Home X</Button>
                            <Button onClick={(done) => this._run_command({ home_y: true }, done)}
                                preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Home Y</Button>
                            <Button onClick={(done) => this._run_command({ home_all: true }, done)}
                                preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Home All</Button>
                            <Button onClick={(done) => this._run_command({ mesh_level: true }, done)}
                                preset="outline-dark" style={{ width: '100%', marginBottom: 5 }}>Mesh Level</Button>
                        </div>
                    </div>


                    <div style={{ paddingTop: 10 }}>
                        <table className="table" style={{ verticalAlign: 'baseline', margin: 0 }}>
                            <thead>
                                <tr>
                                    <th>Axis</th>
                                    <th>Work Position</th>
                                    <th>Machine Position</th>
                                </tr>
                            </thead>
                            <tbody>
                                {machine.config.axes.map((axis) => {
                                    if (axis.type != 'POSITION') {
                                        return null;
                                    }

                                    let machine_pos = axis_values[axis.id][1];

                                    // TODO: Render all the numbers at fixed precision and with right alignment.
                                    return (
                                        <tr key={axis.id}>
                                            <td>{axis.name || axis.id}</td>
                                            <td style={{ fontFamily: 'Noto Sans Mono' }}>
                                                {round_digits(axis_values[axis.id][0], 2)}
                                            </td>
                                            <td style={{ fontFamily: 'Noto Sans Mono' }}>
                                                {machine_pos === undefined ? 'N/A' : round_digits(machine_pos, 2)}
                                            </td>
                                        </tr>
                                    );

                                })}
                                {/*
                                <tr>
                                    <td>Tool</td>
                                    <td colSpan={2}>
                                        <select className="form-select">
                                            <option selected>Index 0</option>
                                            <option value="1">One</option>
                                            <option value="2">Two</option>
                                            <option value="3">Three</option>
                                        </select>
                                    </td>
                                </tr>
                                */}

                            </tbody>
                        </table>
                    </div>
                </CardBody>
            </Card>
        );
    }
};

class JogButtons extends React.Component<{ machine: any, context: PageContext }> {

    state = {
        _increment: 1,
        _feedrate: 1000
    }

    _on_click_arrow = async (axis_id: string, direction: number) => {

        // TODO: Limit how quickly the user can press these buttons.

        let ctx = this.props.context;

        try {
            let res = await ctx.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: this.props.machine.id,
                jog: {
                    feed_rate: this.state._feedrate,
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

                <div style={{ width: 140, display: 'inline-block', marginRight: 10 }}>
                    <div style={{ fontSize: '0.8em' }}>
                        Feedrate (mm/min):
                    </div>
                    <input type="text" className="form-control" value={this.state._feedrate + ''} onChange={(e) => this.setState({ _feedrate: e.target.value * 1 })} />
                </div>

                {/* TODO: Hide if not a milling machine */}
                {false ? (
                    <div style={{ width: 140, display: 'inline-block', marginRight: 10 }}>
                        <div style={{ fontSize: '0.8em' }}>
                            Spindle (RPM):
                        </div>
                        <div className="input-group mb-3">
                            <input type="text" className="form-control" placeholder="10000" />
                            <button className="btn btn-outline-secondary" type="button" id="button-addon2">x</button>
                        </div>
                    </div>
                ) : null}

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
                                {machine.config.axes.map((axis) => {
                                    if (axis.type != 'HEATER' || axis.hide) {
                                        return null;
                                    }

                                    return (
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

                                })}
                            </tbody>
                        </table>
                    </div>
                </CardBody>
            </Card>
        );
    }
};


