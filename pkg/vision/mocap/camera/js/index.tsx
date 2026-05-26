import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";
import { deep_copy, shallow_copy } from "pkg/web/lib/utils";
import { Button } from "pkg/web/lib/button";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";
import { render_property_list } from "pkg/media/camera/js/property";
import { Blob2dViewer } from "./blob_viewer";
import { MocapCameraControls } from "./controls";
import { camera_orientation } from "./orientation";


interface AppState {
    status: any;
    pending_config: any;
    blobs: any;
}

class App extends React.Component<{}, AppState> {

    _mjpeg_img: React.RefObject<HTMLImageElement> = React.createRef();

    state: AppState = {
        status: null,
        pending_config: null,
        blobs: null,
    }

    _channel: Channel = new Channel('/rpc');

    // // If true, we are currently fetching updated preview data for the pages.
    // _fetching_preview: boolean = false;

    constructor(props: {}) {
        super(props);
        this._get_status();
        this._read_blobs();
    }

    _get_status = async () => {
        try {
            let res = await this._channel.call('mocap.MocapCamera', 'Status', {});
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            this.setState({ status: res.responses[0] });

        } catch (e) {
            console.error(e);
        }

        setTimeout(this._get_status, 2000);
    }

    async _get_status_once() {
        let res = await this._channel.call('mocap.MocapCamera', 'Status', {});
        if (!res.status.ok()) {
            throw res.status.toString();
        }

        this.setState({ status: res.responses[0] });
    }

    _read_blobs = async () => {

        // TODO: Retry everything with exponential backoff.

        let res = this._channel.call_streaming('mocap.MocapCamera', 'ReadBlobs', {
            max_rate: 10
        });
        // TODO: Check the response status.

        while (true) {
            let msg = await res.recv();
            if (!msg) {
                // TODO: This is an error
                return;
            }

            // console.log('BLOBS', msg);

            this.setState({ blobs: msg.cameras[0] });
        }


    }

    _set_led_color = (color) => {
        let config = this.state.pending_config;
        if (!config) {
            config = this.state.status.config;
        }

        config = deep_copy(config);

        for (var i = 0; i < config.rgb_led_colors.length; i++) {
            config.rgb_led_colors[i] = color;
        }

        this.setState({ pending_config: config }, () => {
            this._start_update(true);
        });
    }

    // TODO: Need a visual indicator of updating in progress and when it errors out
    _pending_update: boolean = false;
    _start_update = async (immediate = false) => {
        if (this._pending_update) {
            return;
        }

        this._pending_update = true;

        if (!immediate) {
            await new Promise((res, rej) => {
                setTimeout(() => {
                    res()
                }, 500);
            });
        }

        let data = deep_copy(this.state.pending_config);

        try {
            let res = await this._channel.call('mocap.MocapCamera', 'Configure', data);
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            await this._get_status_once();

        } finally {
            this._pending_update = false;

            if (JSON.stringify(data) != JSON.stringify(this.state.pending_config)) {
                this._start_update();
            } else {
                this.setState({ pending_config: null });
            }
        }
    }

    // TODO: The more ideal situation is that we use a smart viewer that reads the MJPEG data stream
    // and can pull out the individual frames without re-encoding.
    _save_frame = (done) => {
        const img = this._mjpeg_img.current;

        // Create a temporary canvas
        const canvas = document.createElement('canvas');

        // Set canvas dimensions to match the actual image resolution
        canvas.width = img.naturalWidth;
        canvas.height = img.naturalHeight;

        // Draw the current image frame onto the canvas
        const ctx = canvas.getContext('2d');
        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

        // Convert the canvas to a JPEG data URL
        const dataURL = canvas.toDataURL('image/jpeg', 1.0); // 1.0 is max quality

        // Create a temporary link element to trigger the download
        const link = document.createElement('a');
        link.href = dataURL;
        link.download = `frame-${Date.now()}.jpg`; // Gives the file a unique name

        // Trigger the download and clean up
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);

        done();
    }

    _render_live_col() {
        let status = this.state.status;

        let orientation = camera_orientation(status);

        // 

        // TODO: max-height: 500 of both of them.
        return (
            <div className="col col-md-8">
                <Card header="Live View (MJPEG)" style={{ marginBottom: 10 }}>
                    <CardBody>
                        <img ref={this._mjpeg_img} src="/camera" style={{
                            maxWidth: 1000,
                            transform: (orientation.upside_down ? 'scaleY(-1)' : 'scaleX(-1)')
                        }} />
                        <Button preset="secondary" onClick={this._save_frame}
                            style={{ display: 'block', marginTop: 10 }}
                        >
                            Save Frame
                        </Button>
                    </CardBody>
                </Card>

                <Card header="Blob view" style={{ marginBottom: 10 }}>
                    <CardBody>
                        <div style={{ maxWidth: 1000 }}>
                            <Blob2dViewer status={status} results={this.state.blobs} upsideDown={orientation.upside_down} />
                        </div>

                    </CardBody>
                </Card>
            </div>
        );

    }

    _render_controls_col() {

        let status = this.state.status;

        let config = this.state.pending_config;
        if (!config) {
            config = this.state.status.config;
        }

        let led_intensity = Math.round(0.5 * 255);
        let led_colors = [
            {
                name: 'Off',
                value: 0
            },
            {
                name: 'Red',
                value: led_intensity << 16
            },
            {
                name: 'Green',
                value: led_intensity << 8
            },
            {
                name: 'Blue',
                value: led_intensity
            },
        ];

        let status_friendly = deep_copy(status);
        delete status_friendly['camera_controls'];

        return (
            <div className="col col-md-4">
                <Card header="LEDs" style={{ marginBottom: 10 }}>
                    <CardBody>
                        <div className="btn-group me-2" role="group">
                            {led_colors.map((v, i) => {
                                // TODO: Check that all the leds are the same color.
                                let value = v.value;
                                let active = config.rgb_led_colors[0] == value;

                                return (
                                    <button key={i} onClick={() => this._set_led_color(value)} type="button" className={"btn " + (active ? 'btn-outline-dark active' : 'btn-outline-secondary')}>{v.name}</button>
                                );
                            })}
                        </div>
                    </CardBody>
                </Card>

                <Card header="Controls" style={{ marginBottom: 10 }}>
                    <CardBody>
                        <MocapCameraControls config={config} camera_controls={status.camera_controls} onChange={(config) => {
                            this.setState({ pending_config: config }, () => {
                                this._start_update();
                            });
                        }} />
                    </CardBody>
                </Card>

                <Card header="Raw Status" style={{ marginBottom: 10 }}>
                    <CardBody>
                        <textarea className="form-control" disabled={true} style={{ minHeight: 150, fontFamily: "Noto Sans Mono" }} value={JSON.stringify(status_friendly, null, 2)}></textarea>
                    </CardBody>
                </Card>

            </div>
        );
    }

    render() {
        if (!this.state.status) {
            return <div>Loading...</div>;
        }

        return (
            <div className="app-outer">
                <div className="container-fluid" style={{ paddingTop: 20, paddingBottom: 20 }}>
                    <div className="row">
                        {this._render_live_col()}
                        {this._render_controls_col()}
                    </div>
                </div>
            </div>
        );
    }
};




let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)