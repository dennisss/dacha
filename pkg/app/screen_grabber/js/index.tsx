import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";
import { deep_copy } from "pkg/web/lib/utils";
import { SpinnerInline } from "pkg/web/lib/spinner";
import { Button } from "pkg/web/lib/button";
import { round_digits } from "pkg/web/lib/formatting";

interface AppState {
    windows: any[] | null;
    selected_window_id: any | null;
    data: any | null;
}

class App extends React.Component<{}, AppState> {

    state: AppState = {
        windows: null,
        selected_window_id: null,
        data: null
    };

    _channel: Channel = new Channel('/rpc');

    constructor(props: {}) {
        super(props);

        this._channel.call('screen_grabber.ScreenGrabber', 'ListWindows', {}).then((res) => {
            let windows = res.responses[0].windows || [];
            let selected_window_id = '';
            if (windows.length > 0) {
                selected_window_id = windows[0].id;
            }

            this.setState({ windows: windows, selected_window_id: selected_window_id });
        });
    }

    _grab = async (done) => {
        try {
            let res = await this._channel.call('screen_grabber.ScreenGrabber', 'Grab', {
                window_id: this.state.selected_window_id,
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }

            this.setState({ data: res.responses[0] });

        } catch (e) {
            console.error(e);
        }

        done();

    }

    _render_grab() {
        let data = this.state.data;

        let image_data = data.image.replaceAll('_', '/').replaceAll('-', '+');

        return (
            <div>
                <div style={{ paddingBottom: 10 }}>
                    <img src={`data:image/png;base64,${image_data}`} style={{ maxWidth: '100%', maxHeight: 400 }}></img>
                </div>
                <div style={{ paddingBottom: 10 }}>
                    <div style={{ border: '1px solid #ccc', padding: 10, maxWidth: 800, fontSize: 20 }}>
                        {data.text || ' '}
                    </div>
                </div>
            </div>
        );
    }

    render() {
        let windows = this.state.windows || [];

        let selected_window_id = this.state.selected_window_id;
        let selected_window = windows.find((win) => win.id == selected_window_id);

        return (
            <div className="app-outer">
                <div className="container">
                    <div style={{ padding: '10px 0' }}>
                        <div className="form-floating" style={{ paddingBottom: 10 }}>
                            <select value={selected_window_id} className="form-select" onChange={(e) => {
                                this.setState({
                                    selected_window_id: e.target.value,
                                });
                            }}>
                                <option value=""></option>
                                {windows.map((window) => {
                                    return (
                                        <option value={window.id} key={window.id}>{window.name}</option>
                                    );
                                })}
                            </select>
                            <label>Window</label>
                        </div>
                        {selected_window ? (
                            <div style={{ paddingBottom: 10 }}>
                                <Button preset="primary" onClick={this._grab}>Grab</Button>
                            </div>
                        ) : null}
                        {this.state.data ? (
                            this._render_grab()
                        ) : null}
                    </div>
                </div>
            </div>
        );
    }
};




let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)