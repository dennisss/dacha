import React from "react";
import ReactDOM from "react-dom";
// import { VideoPlayer } from "./player";
import { Channel } from "pkg/web/lib/rpc";
import { VideoPlayer } from "pkg/web/lib/video";
import { VideoSourceKind } from "pkg/web/lib/video/types";
import { VideoCrosshair } from "pkg/web/lib/video/crosshair";
import { render_group_property } from "pkg/media/camera/js/property";
import { deep_copy, shallow_copy } from "pkg/web/lib/utils";


class App extends React.Component<{}, {}> {
    _channel: Channel = new Channel('/rpc');

    state = {
        _cameras: [],
        _current_camera: '',
        _properties: [],
        _property_states: {},
        _crosshair_size: 0,
        _format: null
    }

    constructor(props: {}) {
        super(props);
        this._load();
    }

    async _load() {
        let res = await this._channel.call('media.camera.CameraInterface', 'ListCameras', {});
        if (!res.status.ok()) {
            throw res.status.toString();
        }

        this.setState({
            _cameras: res.responses[0].entries
        })

        console.log(res.responses);
    }

    async _change_camera(camera_id) {
        let res = await this._channel.call('media.camera.CameraInterface', 'GetProperties', { camera_id });
        if (!res.status.ok()) {
            throw res.status.toString();
        }

        console.log(res.responses);

        let props = res.responses[0].properties;

        let _property_states = {};
        (props.state.states || []).map((s) => {
            _property_states[s.id] = s;
        })

        this.setState({
            _current_camera: camera_id,
            _properties: props.properties,
            _property_states,
            _format: res.responses[0].format
        });
    }

    _render_video_player() {
        if (!this.state._current_camera) {
            return null;
        }

        let format = this.state._format.live_format;

        let kind = VideoSourceKind.Live;
        if (format == 'MJPEG') {
            kind = VideoSourceKind.MJPEG;
        }

        return (
            <VideoPlayer source={{
                kind,
                url: `/camera/` + encodeURIComponent(this.state._current_camera)
            }}>
                <VideoCrosshair size={this.state._crosshair_size} />
            </VideoPlayer>
        );
    }

    _on_property_change = (prop, value) => {
        let states = shallow_copy(this.state._property_states);
        states[prop.id] = {
            id: prop.id,
            current_value: value
        };

        this.setState({ _property_states: states }, () => {
            this._start_update()
        });
    }

    // TODO: Need a visual indicator of updating in progress and when it errors out
    _pending_update: boolean = false;
    _start_update = async () => {
        if (this._pending_update) {
            return;
        }

        this._pending_update = true;

        await new Promise((res, rej) => {
            setTimeout(() => {
                res()
            }, 100);
        });

        let camera_id = this.state._current_camera
        let states = deep_copy(this.state._property_states);

        try {
            let res = await this._channel.call('media.camera.CameraInterface', 'SetProperties', { camera_id, state: { states: Object.values(states) } });
            if (!res.status.ok()) {
                throw res.status.toString();
            }

        } finally {
            this._pending_update = false;

            if (camera_id != this.state._current_camera || JSON.stringify(states) != JSON.stringify(this.state._property_states)) {
                this._start_update();
            }
        }
    }

    render() {
        return (
            <div style={{ position: 'fixed', top: 0, left: 0, right: 0, bottom: 0, display: 'flex' }}>
                <div style={{ width: 500, padding: 10, overflowY: 'scroll' }}>
                    <select value={this.state._current_camera} className="form-control" onChange={(e) => this._change_camera(e.target.value)}>
                        <option value=""></option>
                        {this.state._cameras.map((c) => {
                            return <option key={c.id} value={c.id}>{c.name}</option>;
                        })}
                    </select>

                    <div style={{ paddingTop: 20, fontSize: '0.8em' }}>
                        {this.state._properties.map((prop) => {
                            return render_group_property(prop, this.state._property_states, this._on_property_change);
                        })}
                    </div>

                </div>

                <div style={{ flexShrink: 10000, padding: 10 }}>
                    {this._render_video_player()}

                    <div>
                        Crosshair Size
                        <input type="number" className="form-control" min={0} max={1000} value={this.state._crosshair_size} onChange={(e) => {
                            this.setState({ _crosshair_size: e.target.value * 1 });
                        }} />
                    </div>
                </div>
            </div>
        );
    }
};


let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)