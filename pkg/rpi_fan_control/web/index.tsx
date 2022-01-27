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
        this._channel.call("FanControl", "Read", {})
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
        this._channel.call("FanControl", "Write", this.state._proto)
            .finally(() => {
                this._updating = false;
                if (this._pending_update) {
                    this._pending_update = false;
                    this._write_proto();
                }
            });
    }

    render() {
        if (!this.state._proto) {
            return <div style={{ padding: 20 }}>Loading..</div>;
        }

        let fan_speed = Math.round((this.state._proto.current_speed || 0) * 100);
        let temp = Math.round(100 * (this.state._proto.current_temp || 0) / 100);
        let auto = this.state._proto.auto || false;

        return (
            <div style={{ padding: 20 }}>
                <table className="table">
                    <tbody>
                        <tr>
                            <td>CPU Temperature</td>
                            <td>{temp} &deg;C</td>
                        </tr>
                        <tr>
                            <td>Fan Speed</td>
                            <td>
                                <input type="range" className="form-range"
                                    min={0} max={100} step={1} value={fan_speed} disabled={auto}
                                    onChange={(e) => this._update((proto) => {
                                        proto.current_speed = e.target.valueAsNumber / 100;
                                    })} />
                            </td>
                            <td>
                                {fan_speed}%
                            </td>
                        </tr>
                        <tr>
                            <td>Auto</td>
                            <td>
                                <input className="form-check-input" type="checkbox" checked={auto}
                                    onChange={(e) => this._update((proto) => {
                                        proto.auto = e.target.checked;
                                    })} />
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        );
    }
};


let node = document.getElementById("app-root");
console.log("Place in", node);
ReactDOM.render(<App />, node)