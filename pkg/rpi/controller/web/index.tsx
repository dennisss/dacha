import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";

class App extends React.Component<{}, { _proto?: any }> {
    _channel: Channel;

    _updating: boolean = false;
    _pending_update: boolean = false;

    constructor(props: {}) {
        super(props);
        this.state = {
            _proto: null
        };

        this._channel = new Channel(`${window.location.protocol}//${window.location.hostname}:${global.vars.rpc_port}`);
    }

    componentDidMount() {
        // Periodically fetch the latest state. 
        this._refresh();
    }

    _refresh() {
        // TODO: Track the staleness of the data and display an error about it.
        this._channel.call("rpi.Controller", "Read", {})
            .then((res) => {
                if (!this._updating) {
                    this.setState({ _proto: res.responses[0] })
                }
            })
            .finally(() => {
                setTimeout(() => this._refresh(), 2000);
            });
    }

    _update(applier: (state: any) => void) {
        let new_proto = JSON.parse(JSON.stringify(this.state._proto));
        applier(new_proto);


        this.setState({ _proto: new_proto }, () => {
            this._write_proto();
        });
    }

    _write_proto() {
        if (this._updating) {
            this._pending_update = true;
            return;
        }

        this._updating = true;
        this._channel.call("rpi.controller.FanControl", "Write", this.state._proto)
            .finally(() => {
                this._updating = false;
                if (this._pending_update) {
                    this._pending_update = false;
                    this._write_proto();
                }
            });
    }

    _on_identify = () => {
        this._channel.call("rpi.controller.FanControl", "Identify", {})
            .finally(() => {
                console.warn('Done identify!');
            });
    }

    render() {
        if (!this.state._proto) {
            return <div style={{ padding: 20 }}>Loading..</div>;
        }

        let state_map = {};
        this.state._proto.state.entities.map((s) => {
            state_map[s.id] = s.value;
        });

        return (
            <div className="container" style={{ paddingTop: 20, paddingBottom: 20 }}>
                {this.state._proto.config.entities.map((entity) => {
                    return <EntityCard key={entity.id} id={entity.id} state={state_map[entity.id]} config={entity.value} getChannel={() => this._channel} />
                })}
            </div>
        );
    }
};

class EntityCard extends React.Component<{ id: string, config: any, state: any, getChannel: any }, { _pending_state: any | null, _pending_state_json: string | null, _pending_state_json_valid: boolean, _updating: boolean, _error_message: string | null }> {

    constructor(props: any) {
        super(props);
        this.state = {
            // If non-null, then this is the next proposed value of the state (not yet sent to the server). 
            _pending_state: null,

            _pending_state_json: null,
            _pending_state_json_valid: true,

            // Whether or not we are currently waiting for pending_state to be send to the server.
            _updating: false,
            _error_message: null,
        };
    }

    _onStateTextChange = (e: any) => {
        let raw_text = e.target.value;

        let update: any = {};
        try {
            update._pending_state = JSON.parse(raw_text);
            update._pending_state_json_valid = true;
        } catch (e) {
            console.error(e);
            update._pending_state_json_valid = false;
        }

        update._pending_state_json = raw_text;

        this.setState(update);
    }

    _updateProposedState = (f: any, trigger_update: boolean = false) => {
        if (this.state._updating) {
            return;
        }

        let { state } = this.props;
        if (this.state._pending_state !== null) {
            state = this.state._pending_state;
        }

        // Deep clone
        state = JSON.parse(JSON.stringify(state));

        f(state);

        this.setState({
            _pending_state: state,
            _pending_state_json_valid: true,
            _pending_state_json: JSON.stringify(state, null, 4),
        }, () => {
            if (trigger_update) {
                this._onClickUpdate();
            }
        });
    }

    _onClickUpdate = () => {
        if (this.state._updating) {
            return false;
        }

        this.setState({ _updating: true });

        let channel: Channel = this.props.getChannel();

        // ControllerState proto
        let update = {
            entities: [{
                id: this.props.id,
                value: this.state._pending_state
            }]
        }

        channel.call("rpi.Controller", "Write", update)
            .then((res) => {
                if (!res.status.ok()) {
                    this.setState({
                        _updating: false,
                        _error_message: res.status.toString()
                    });
                    return;
                }

                // TODO: Need this to immediately trigger an update of the state.
                this._onClickCancel();
            })
            .catch((e) => {
                this.setState({
                    _updating: false,
                    _error_message: e + ""
                });
            });
    }

    _onClickCancel = () => {
        this.setState({
            _pending_state: null,
            _pending_state_json: null,
            _pending_state_json_valid: true,
            _updating: false,
            _error_message: null
        })
    }

    _onLEDIdentity = () => {
        let now = (new Date()).getTime();
        let second = 1000;
        this._updateProposedState((proto) => {
            proto.periods = [
                {
                    start_time: now + 2 * second,
                    end_time: now + 4 * second,
                    color: [0x0000FF]
                },
                {
                    start_time: now + 6 * second,
                    end_time: now + 8 * second,
                    color: [0x0000FF]
                },
                {
                    start_time: now + 10 * second,
                    end_time: now + 12 * second,
                    color: [0x0000FF]
                }
            ];
        }, true);
    }

    render() {
        let type_specific_view = null;

        let { config, state } = this.props;

        if (this.state._pending_state !== null) {
            state = this.state._pending_state;
        }

        let have_pending_change = this.state._pending_state !== null;
        let state_json = this.state._pending_state_json !== null ? this.state._pending_state_json : JSON.stringify(state, null, 4);

        if (state['@type'] == 'type.googleapis.com/rpi.TemperatureSensorState') {
            let temp = Math.round(100 * (state['temperature'] || 0) / 100);
            type_specific_view = (
                <div>CPU Temperature: {temp} &deg;C</div>
            );
        }

        if (state['@type'] == 'type.googleapis.com/rpi.FanCurveState') {
            type_specific_view = (
                <table className="table" style={{ width: '100%' }}>
                    <tbody>
                        <tr>
                            <td style={{ width: 200 }}>Auto</td>
                            <td>
                                <input className="form-check-input" type="checkbox" checked={state.enabled ? true : false}
                                    onChange={(e) => this._updateProposedState((proto: any) => {
                                        proto.enabled = e.target.checked;
                                    })} />
                            </td>
                        </tr>
                    </tbody>
                </table>
            );
        }

        if (state['@type'] == 'type.googleapis.com/rpi.FanState') {
            let fan_speed = Math.round((state.target_speed || 0) * 100);

            type_specific_view = (
                <table className="table" style={{ width: '100%' }}>
                    <tbody>
                        <tr>
                            <td style={{ width: 200 }}>Target Speed</td>
                            <td>
                                {fan_speed}%
                            </td>
                            <td>
                                <input type="range" className="form-range"
                                    min={0} max={100} step={1} value={fan_speed}
                                    onChange={(e) => this._updateProposedState((proto: any) => {
                                        proto.target_speed = e.target.valueAsNumber / 100;
                                    })}
                                />
                            </td>
                        </tr>
                        <tr>
                            <td>Measured RPM</td>
                            <td>
                                {state.measured_rpm}
                            </td>
                        </tr>
                    </tbody>
                </table>
            );

        }

        if (state['@type'] == 'type.googleapis.com/rpi.WS2812StripState') {
            type_specific_view = (
                <button className="btn btn-primary" onClick={this._onLEDIdentity}>Identify</button>
            );
        }

        return (
            <div className="card" style={{ marginBottom: 20 }}>
                <div className="card-header">
                    Id: {this.props.id}
                </div>
                <div className="card-body">
                    {type_specific_view ? <>{type_specific_view}<hr style={{ marginRight: -16, marginLeft: -16 }} /></> : null}
                    <table className="table">
                        <tbody>
                            <tr>
                                <th style={{ width: '50%' }}>Config</th>
                                <th>State</th>
                            </tr>
                            <tr>
                                <td style={{ verticalAlign: 'top' }}>
                                    <textarea className="form-control" disabled={true} style={{ minHeight: 150, fontFamily: "Noto Sans Mono" }} value={JSON.stringify(config, null, 2)}></textarea>
                                </td>
                                <td style={{ verticalAlign: 'top' }}>
                                    <textarea onChange={this._onStateTextChange} className="form-control" style={{ minHeight: 150, borderColor: this.state._pending_state_json_valid ? 'green' : 'red', fontFamily: "Noto Sans Mono" }} value={state_json}></textarea>
                                </td>
                            </tr>
                        </tbody>
                    </table>
                    {have_pending_change ? (
                        <div style={{ textAlign: 'right' }}>
                            <button type="button" className="btn btn-light" onClick={this._onClickCancel} style={{ marginRight: 20 }}>Cancel</button>
                            <button type="button" className="btn btn-primary" onClick={this._onClickUpdate} disabled={this.state._updating}>
                                {this.state._updating ? "Loading..." : "Apply"}
                            </button>
                        </div>
                    ) : null}
                    {this.state._error_message ? (
                        <div className="alert alert-danger" style={{ marginTop: 10, marginBottom: 0 }}>
                            <div>
                                {this.state._error_message}
                            </div>
                        </div>
                    ) : null}
                </div>
            </div>
        );
    }

}


let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)