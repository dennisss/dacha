import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";

interface AppState {

}

class App extends React.Component<{}, AppState> {

    state: AppState = {
        config: null,
        state: null,
    };

    _channel: Channel = new Channel('/rpc');

    constructor(props: {}) {
        super(props);

        this._channel.call('peripherals.Peripherals', 'GetConfig', {}).then((res) => {
            let config = res.responses[0].config || {};
            let state = res.responses[0].state || {};
            this.setState({ config, state });
        });
    }

    _execute(req) {
        this._channel.call('peripherals.Peripherals', 'Execute', req).then((res) => {
            // TODO: Dump to a text box.
            console.log(res.responses[0]);
            let state = res.responses[0].state || {};
            this.setState({ state });
        });
    }

    _render_peripheral(periph) {

        let state = null;
        (this.state.state.states || []).map((s) => {
            if ((s.index || 0) === (periph.index || 0)) {
                state = s;
            }
        });

        let control = null;

        if (periph.gpio) {
            let high = state.gpio.high || false;
            control = <input type="checkbox" checked={high}
                onChange={(e) => this._execute({ peripheral_index: (periph.index || 0), set_gpio_level: { high: !high } })} />
        }
        if (periph.pwm) {
            let max_val = 65536 - 1;
            let value = Math.round(100 * (state.pwm.value || 0) / max_val);

            control = (
                <input type="range" className="form-range"
                    min={0} max={100} step={1} value={value}
                    onChange={(e) => {
                        let new_value = Math.round(max_val * (e.target.valueAsNumber / 100));
                        this._execute({ peripheral_index: (periph.index || 0), set_pwm: { value: new_value } });
                    }}
                />
            );

        }

        return (
            <tr key={periph.name}>
                <td style={{ width: 1, whiteSpace: 'nowrap' }}>
                    {periph.name}
                </td>
                <td>
                    {control}
                </td>
            </tr>
        );
    }

    _render_macro(m) {

        return (
            <button className="btn btn-primary" key={m.name} style={{ marginRight: 10 }} onClick={(e) => {
                this._channel.call('peripherals.Peripherals', 'RunMacro', { name: m.name }).then((res) => {
                    let state = res.responses[0].state || {};
                    this.setState({ state });
                });
            }}>
                {m.name}
            </button>
        );

    }

    render() {
        let config = this.state.config;
        if (!config) {
            return <div></div>;
        }

        return (
            <div className="app-outer">
                <div className="container">
                    <div style={{ padding: '10px 0' }}>
                        <div>
                            {(config.macros || []).map((p) => this._render_macro(p))}
                        </div>

                        <table className="table">
                            <tbody>
                                {(config.peripherals || []).map((p) => this._render_peripheral(p))}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        );
    }
};




let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)