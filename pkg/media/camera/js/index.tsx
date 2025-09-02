import React from "react";
import ReactDOM from "react-dom";
// import { VideoPlayer } from "./player";
import { Channel } from "pkg/web/lib/rpc";
import { VideoPlayer } from "pkg/web/lib/video";
import { VideoSourceKind } from "pkg/web/lib/video/types";
import { VideoCrosshair } from "pkg/web/lib/video/crosshair";


class App extends React.Component<{}, {}> {
    _channel: Channel = new Channel('/rpc');

    state = {
        _cameras: [],
        _current_camera: '',
        _properties: [],
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

        this.setState({ _current_camera: camera_id, _properties: res.responses[0].properties.properties, _format: res.responses[0].format });
    }

    _render_property(prop) {
        if (!prop.spec || prop.spec.type != 'GROUP') {
            return <div key={prop.id}>Unknown: {prop.id}</div>;
        }

        return (
            <div key={prop.id} className="card" style={{ marginBottom: 10 }}>
                <div className="card-header">
                    {prop.spec.name || prop.id}
                </div>
                <div className="card-body" style={{ padding: 10 }}>
                    <div style={{ wordBreak: 'break-all' }}>
                        <table className="table">
                            <tbody>
                                {(prop.children || []).map((prop) => {
                                    if (prop.spec && prop.spec.type == 'GROUP') {
                                        return null;
                                    }

                                    return (
                                        <tr key={prop.id}>
                                            <td style={{ whiteSpace: 'nowrap', width: 1, verticalAlign: 'baseline' }}>
                                                {prop.spec.name || prop.id}
                                            </td>
                                            <td style={{ verticalAlign: 'baseline' }}>
                                                <div style={{ width: '100%', overflowX: 'hidden' }}>
                                                    {this._render_property_value(prop)}
                                                </div>
                                            </td>
                                        </tr>
                                    )
                                })}
                            </tbody>
                        </table>
                    </div>


                    {(prop.children || []).map((prop) => {
                        // Rendering nested groups outside of the table.

                        if (prop.spec && prop.spec.type != 'GROUP') {
                            return null;
                        }

                        return this._render_property(prop);
                    })}
                </div>
            </div>
        );
    }

    _render_property_value(prop) {
        prop.spec = prop.spec || {};


        if (prop.spec.values || prop.spec.type == 'ENUM') {

            return (
                <select className="form-control" style={{ fontSize: '0.8em' }} value="">
                    {(prop.spec.values || []).map((value, i) => {
                        return (
                            <option key={i}>{value.value_name}</option>
                        );
                    })}
                </select>

            );
        }

        if (prop.spec.type == 'BOOL') {
            return (
                <input type="checkbox" checked={false} />
            );
        }

        if (prop.spec.type == 'INT32') {
            return (
                <div style={{ display: "flex" }}>
                    <div style={{ flexGrow: 1 }}>
                        <input style={{ width: '100%' }} type="range"
                            min={prop.spec.min_value.int32_value || 0}
                            max={prop.spec.max_value.int32_value || 0}
                            step={prop.spec.step.int32_value || 0}
                            value={prop.spec.default_value.int32_value || 0} />
                    </div>
                    <div style={{ width: 100 }}>
                        <input className="form-control" type="number" value={prop.spec.default_value.int32_value || 0} />
                    </div>
                </div>
            );
        }

        /*
        GROUP = 1;
        BOOL = 2;
        // For this type of value, the int32_value is used.
        ENUM = 3;
        INT32 = 4;
        FLOAT32 = 5;

        */


        return (
            <div>Unknown {prop.spec.type}</div>
        )
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
                kind: VideoSourceKind.MJPEG,
                url: `/camera/` + encodeURIComponent(this.state._current_camera)
            }}>
                <VideoCrosshair size={this.state._crosshair_size} />
            </VideoPlayer>
        );
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
                            return this._render_property(prop);
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