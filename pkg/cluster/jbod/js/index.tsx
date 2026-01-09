import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";
import { round_digits } from "pkg/web/lib/formatting";
import { Button } from "pkg/web/lib/button";
import { NotificationsComponent, NotificationStore } from "pkg/web/lib/notifications";
import { compare_values } from "pkg/web/lib/utils";

class App extends React.Component<{}, { _proto?: any }> {
    _channel: Channel;
    _notifications: NotificationStore;

    _updating: boolean = false;
    _pending_update: boolean = false;

    constructor(props: {}) {
        super(props);
        this.state = {
            _proto: null
        };

        this._notifications = new NotificationStore();

        this._channel = new Channel('/rpc');
    }

    componentDidMount() {
        // Periodically fetch the latest state. 
        this._refresh();
    }

    _refresh() {
        // TODO: Track the staleness of the data and display an error about it.
        this._channel.call("cluster.Enclosure", "GetState", {})
            .then((res) => {
                if (!this._updating) {
                    this.setState({ _proto: res.responses[0] })
                }
            })
            .finally(() => {
                setTimeout(() => this._refresh(), 500);
            });
    }

    async _run_command(command, done) {
        try {
            let res = await this._channel.call('cluster.Enclosure', 'Execute', command);

            if (!res.status.ok()) {
                throw res.status.toString();
            }
        } catch (e) {
            this._notifications.add({
                text: 'Failure: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        done();
    }

    render() {
        if (!this.state._proto) {
            return <div style={{ padding: 20 }}>Loading..</div>;
        }

        let proto = this.state._proto;


        let psu_rows = (proto.psus || []).map((psu) => {

            let power_button;

            // toggle_psu { psu_name: "left" on: true sas_on: true }

            if (psu.waiting_for_power_on) {
                power_button = (
                    <Button preset="primary" onClick={(done) => this._run_command({
                        toggle_psu: {
                            psu_name: psu.name,
                            on: true,
                            sas_on: psu.sas_on
                        }
                    }, done)}>Turn On</Button>
                );
            } else if (psu.on) {
                power_button = (
                    <Button preset="outline-dark" onClick={(done) => this._run_command({
                        toggle_psu: {
                            psu_name: psu.name,
                            on: false,
                            sas_on: psu.sas_on
                        }
                    }, done)}>Turn Off</Button>
                );
            } else {
                power_button = (
                    <Button preset="outline-dark" disabled={true} onClick={(done) => { }}>Turn On</Button>
                );
            }

            let sas_button;

            if (psu.output_stable && !psu.sas_on) {
                sas_button = (
                    <Button preset="primary" onClick={(done) => this._run_command({
                        toggle_psu: {
                            psu_name: psu.name,
                            on: psu.on,
                            sas_on: true
                        }
                    }, done)}>Turn On</Button>
                );
            } else if (psu.sas_on) {
                sas_button = (
                    <Button preset="outline-dark" onClick={(done) => this._run_command({
                        toggle_psu: {
                            psu_name: psu.name,
                            on: psu.on,
                            sas_on: false
                        }
                    }, done)}>Turn Off</Button>
                );
            } else {
                sas_button = (
                    <Button preset="outline-dark" disabled={true} onClick={(done) => { }}>Turn On</Button>
                );
            }

            return (
                <tr key={psu.name}>
                    <td>{psu.name}</td>
                    <td>{round_digits(psu.voltage_5 || 0, 2)}</td>
                    <td>{round_digits(psu.voltage_12 || 0, 2)}</td>
                    <td>{round_digits(psu.voltage_ps_on || 0, 2)}</td>
                    <td style={{ width: 1, whiteSpace: 'nowrap' }}>{power_button}</td>
                    <td style={{ width: 1, whiteSpace: 'nowrap' }}>{sas_button}</td>
                </tr>
            )
        });

        function storage_dev_full_name(dev) {
            // TODO: Use the complete parent path
            if (dev.parent) {
                return dev.parent + '/' + dev.name;
            } else {
                return dev.name;
            }
        }

        let storage_devices = proto.storage_devices || [];
        storage_devices.sort((a, b) => {
            return compare_values(storage_dev_full_name(a), storage_dev_full_name(b));
        });

        let device_rows = storage_devices.map((device) => {

            let name = storage_dev_full_name(device);
            if (device.position) {
                name += ' (' + device.position + ')';
            }

            return (
                <tr key={device.name}>
                    <td>{name}</td>
                    <td>{device.usage}</td>
                    <td>{device.temperature || '?'}</td>
                    <td>{device.disk_stats ? device.disk_stats.smart_status : ''}</td>
                    <td>{device.disk_stats ? (device.disk_stats.read_soft_errors || 0) : ''}</td>
                    <td>{device.disk_stats ? (device.disk_stats.read_hard_errors || 0) : ''}</td>
                    <td>{device.disk_stats ? (device.disk_stats.write_soft_errors || 0) : ''}</td>
                    <td>{device.disk_stats ? (device.disk_stats.write_hard_errors || 0) : ''}</td>
                </tr>
            )
        });


        let draw_fan = (text, name) => {

            let speed = 0;
            ((proto.fan_groups || [{}])[0].fans || []).map((fan) => {
                if (fan.name == name) {
                    speed = fan.measured_speed || 0;
                }
            })

            return (
                <ComponentMapCell colSpan={5}>
                    {name}
                    <br />
                    {round_digits(speed, 0)} RPM
                </ComponentMapCell>
            );
        };

        let led_mode = (proto.leds || {}).mode || 'UNKNOWN';


        return (
            <div className="container" style={{ paddingTop: 20, paddingBottom: 20 }}>
                <NotificationsComponent notifications={this._notifications} />

                <Card header="Component Map" style={{ marginBottom: 10 }}>
                    <CardBody>
                        <table>
                            <tbody>
                                <tr>
                                    {draw_fan('Back Left Fan', 'back_left')}
                                    {draw_fan('Back Middle Fan', 'back_middle')}
                                    {draw_fan('Back Right Fan', 'back_right')}
                                </tr>
                                {(() => {
                                    let out = [];

                                    let bays = (proto.bays || [])

                                    let bay_i = 0;
                                    while (bay_i < bays.length) {

                                        let cols = [];

                                        for (var i = 0; i < 15; i++) {
                                            if (bay_i >= bays.length) {
                                                break;
                                            }

                                            let bay = bays[bay_i];

                                            cols.push(
                                                <ComponentMapCell key={bay_i}>
                                                    {bay.connected_device_name || <span>&nbsp;</span>}
                                                </ComponentMapCell>
                                            );

                                            bay_i += 1;
                                        }

                                        out.push(<tr>{cols}</tr>)
                                    }

                                    return out;
                                })()}

                                <tr>
                                    {draw_fan('Front Left Fan', 'front_left')}
                                    {draw_fan('Front Middle Fan', 'front_middle')}
                                    {draw_fan('Front Right Fan', 'front_right')}
                                </tr>

                            </tbody>
                        </table>

                    </CardBody>
                </Card>

                <Card header="Power Supplies" style={{ marginBottom: 10 }}>
                    <div style={{ padding: '0 8px' }}>
                        <table className="table table-hover" style={{ verticalAlign: "baseline" }}>
                            <thead>
                                <tr>
                                    <th>Name</th>

                                    <th>5V</th>
                                    <th>12V</th>
                                    <th>PS_ON</th>
                                    <th style={{ width: 1, whiteSpace: 'nowrap' }}>Main Power</th>
                                    <th style={{ width: 1, whiteSpace: 'nowrap' }}>SAS Power</th>
                                </tr>
                            </thead>
                            <tbody>
                                {psu_rows}
                            </tbody>
                        </table>
                    </div>
                </Card>

                <Card header="Devices" style={{ marginBottom: 10 }}>
                    <div style={{ padding: '0 8px' }}>
                        <table className="table table-hover" style={{ verticalAlign: "baseline" }}>
                            <thead>
                                <tr>
                                    <th>Name</th>
                                    <th>Usage</th>
                                    <th>Temp (°C)</th>
                                    <th>SMART</th>
                                    <th>Read Error<br />Soft</th>
                                    <th>Read Error<br />Hard</th>
                                    <th>Write Error<br />Soft</th>
                                    <th>Write Error<br />Hard</th>
                                </tr>
                            </thead>
                            <tbody>
                                {device_rows}
                            </tbody>
                        </table>
                    </div>
                </Card>

                <Card header="LEDs" style={{ marginBottom: 10 }}>
                    <CardBody>
                        <select className="form-control" value={led_mode} onChange={(e) => {
                            this._run_command({
                                set_led_state: {
                                    mode: e.target.value,
                                }
                            }, () => { });
                        }}>
                            <option value="UNKNOWN" disabled>UNKNOWN</option>
                            <option value="OFF">OFF</option>
                            <option value="STATUS_ID">STATUS_ID</option>
                        </select>
                    </CardBody>
                </Card>

            </div>
        );
    }
};

class ComponentMapCell extends React.Component {
    render() {
        return (
            <td style={{ padding: 10 }} colSpan={this.props.colSpan}>
                <div style={{ border: '1px solid #444', textAlign: 'center', padding: 10, fontSize: '0.8em', ...this.props.style }}>
                    {this.props.children}
                </div>
            </td>
        );
    }
}


let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)