import React from "react";
import { Router } from "pkg/web/lib/router";
import { PageContext } from "pkg/web/lib/page";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "./navbar";
import { compare_values, deep_copy } from "pkg/web/lib/utils";
import { Button } from "pkg/web/lib/button";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";
import { Blob2dViewer } from "../../camera/js/blob_viewer";
import { round_digits } from "pkg/web/lib/formatting";
import { MocapCameraControls } from "../../camera/js/controls";
import { camera_orientation } from "../../camera/js/orientation";


/**
 * Converts a stringified u64 decimal integer to a base32 string.
 * @param {string} idStr - The stringified integer (e.g., "1234567890123456")
 * @returns {string | null}
 */
export function entity_id_to_string(idStr) {
    let num = BigInt(idStr);
    const ENCODE_MAP = "0123456789abcdefghjkmnpqrstvwxyz";
    let out = "";

    // Extract the first 4 bits (0b1111)
    let firstCode = Number(num & 15n);
    num >>= 4n;

    if (firstCode < 10) {
        firstCode |= (1 << 4);
    }

    out += ENCODE_MAP[firstCode];

    // Extract the remaining 60 bits in twelve 5-bit chunks (13 chars total)
    for (let i = 0; i < 12; i++) {
        const code = Number(num & 31n); // 0b11111
        num >>= 5n;
        out += ENCODE_MAP[code];
    }

    if (!/^[a-zA-Z]/.test(out)) {
        return null;
    }

    return out;
}

export interface CamerasPageProps {
    context: PageContext,
}

interface CamerasPageState {
    status: any,
    blobs: any,
}

export class CamerasPage extends React.Component<CamerasPageProps, CamerasPageState> {

    state = {
        status: null,
        blobs: null,
        pending_config: null
    }

    constructor(props: CamerasPageProps) {
        super(props);
        this._get_status();
        this._read_blobs();
    }

    _get_status = async () => {
        try {
            await this._get_status_once();
        } catch (e) {
            console.error(e);
        }

        setTimeout(this._get_status, 2000);
    }

    async _get_status_once() {
        let res = await this.props.context.channel.call('mocap.MocapManager', 'Status', {});
        if (!res.status.ok()) {
            throw res.status.toString();
        }

        this.setState({ status: res.responses[0] });
    }

    _read_blobs = async () => {

        // TODO: Retry everything with exponential backoff.

        let res = this.props.context.channel.call_streaming('mocap.MocapManager', 'ReadBlobs', {
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

            this.setState({ blobs: msg });
        }
    }

    _execute = async (req, done) => {

        try {
            let res = await this.props.context.channel.call('mocap.MocapManager', 'Execute', req);
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            await this._get_status_once();

        } finally {
            done()
        }
    }

    _render_camera_status_row(camera) {
        let ptp_error = '?';
        let ptp_age = 1000;

        let pps_error = '?';
        let pps_age = 1000;

        // TODO: Also want to highlight any error values that are too large (not just based on age)e

        function format_small_time(v) {
            let v_abs = Math.abs(v);

            if (v_abs >= 1) {
                return round_digits(v, 2) + 's';
            }

            if (v_abs >= 0.0001) {
                return round_digits(v * 1000, 2) + 'ms';
            }

            if (v_abs >= (1 / 10000000)) {
                return round_digits(v * 1000000, 2) + 'us';
            }

            return round_digits(v * 1000000000, 2) + 'ns';

        }

        function color_format(value, age) {
            let color = null;
            if (age > 5) {
                color = 'orange';
            }
            if (age > 10) {
                color = 'red';
            }

            return <span style={{ color }}>{value}</span>
        }

        if (camera.synced) {
            let ptp_role = camera.ptp_status.config.role;
            if (ptp_role == 'LEADER') {
                ptp_age = 0;
                ptp_error = 'Leader';
            } else if (camera.ptp_status.follower && camera.ptp_status.follower.got_sync) {
                // TODO: This age seems to be wrong as we don't seem to not failing chains of events.
                ptp_age = (camera.last_sync_age || 0) + (camera.ptp_status.last_sync_age || 0);
                ptp_error = format_small_time(camera.ptp_status.follower.last_leader_error || 0);
            }

            if (camera.camera_status.pps_divider_telemetry) {
                pps_age = (camera.last_sync_age || 0); // TODO: Setup a telemetry age
                pps_error = format_small_time(camera.camera_status.pps_divider_telemetry.prediction_error || 0);
            }
        }

        let active = camera.active || false;

        let latency = (
            <span style={{ color: 'red' }}>-</span>
        );
        if (this.state.blobs) {
            (this.state.blobs.cameras || []).map((c) => {
                if (c.camera_id == camera.id) {
                    latency = (c.latency || '0') * 1;
                    latency = latency / 1000000000;
                    latency = format_small_time(latency);
                }
            });
        }

        return (
            <tr key={camera.id} className={active ? 'table-active' : ''} style={{ cursor: "pointer" }}

                onClick={async () => {
                    // TODO: Error monitoring.
                    await this.props.context.channel.call('mocap.MocapManager', 'Execute', {
                        select_camera: (active ? 0 : camera.id)
                    });
                    this._get_status_once()
                }}
            >
                <td>
                    <a href={"https://" + entity_id_to_string(camera.id) + ".mocap_camera.worker.home.cluster.internal"}>
                        {entity_id_to_string(camera.id)}
                    </a>
                </td>
                <td>{color_format(ptp_error, ptp_age)}</td>
                <td>{color_format(pps_error, pps_age)}</td>
                <td>{latency}</td>
                <td></td>
            </tr>
        )
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
            let res = await this.props.context.channel.call('mocap.MocapManager', 'Execute', {
                configure_cameras: data
            });
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

    _render_blob_views() {
        let status = this.state.status;

        let frame_text = 'Frame: ?';
        if (this.state.blobs) {
            frame_text = 'Frame: ' + this.state.blobs.frame_timestamp;

            let frame_cams = (this.state.blobs.cameras || []).length;
            let total_nums = status.cameras.length;

            frame_text += ' | ' + frame_cams + ' / ' + total_nums + ' cameras'
        }

        return (
            <div className="col col-md-8">
                <div style={{ fontFamily: "Noto Sans Mono", marginBottom: 10, fontSize: 12 }}>
                    {frame_text}
                </div>

                <div style={{ marginBottom: 10, marginLeft: -5, marginRight: -5 }}>
                    {(status.cameras || []).map((camera) => {
                        if (!status.groups) {
                            return null;
                        }

                        let active = camera.active || false;

                        let group = status.groups[0];


                        let blobs = null;
                        if (this.state.blobs) {
                            (this.state.blobs.cameras || []).map((c) => {
                                if (c.camera_id == camera.id) {
                                    blobs = c;
                                }
                            });
                        }

                        let upside_down = false;
                        let angle = 0;
                        if (camera.camera_status) {
                            let o = camera_orientation(camera.camera_status);
                            upside_down = o.upside_down;
                            angle = o.horizon_angle;
                        }


                        return (
                            <div key={camera.id}
                                style={{
                                    opacity: (blobs !== null ? 1 : 0.5),
                                    cursor: 'pointer',
                                    width: '25%',
                                    display: 'inline-block',
                                    padding: 5,
                                    fontFamily: "Noto Sans Mono"
                                }}
                                onClick={async () => {
                                    // TODO: Error monitoring.
                                    await this.props.context.channel.call('mocap.MocapManager', 'Execute', {
                                        select_camera: (active ? 0 : camera.id)
                                    });
                                    this._get_status_once()
                                }}
                            >
                                <div style={{
                                    border: ('1px solid ' + (active ? '#08f' : '#ccc')),
                                    borderRadius: 5,
                                    overflow: 'hidden',
                                    boxShadow: (active ? '0px 0px 10px #08f' : '')
                                }}>
                                    <div style={{
                                        padding: '5px 10px',
                                        backgroundColor: (active ? '#08f' : '#444'),
                                        color: '#fff'
                                    }}>
                                        {entity_id_to_string(camera.id)}

                                        <span style={{ float: 'right', fontSize: '0.9em' }}>
                                            {angle}&deg;
                                        </span>
                                    </div>
                                    <div style={{ fontSize: 0 }}>
                                        <Blob2dViewer status={group} results={blobs} upsideDown={upside_down} />
                                    </div>
                                </div>
                            </div>
                        );
                    })}
                </div>

            </div>

        );


    }

    _render_controls() {

        let status = this.state.status;
        if (!status.groups) {
            return;
        }

        let group = status.groups[0];

        if (!group.config || !group.camera_controls) {
            return null;
        }

        let config = group.config;
        if (this.state.pending_config) {
            config = this.state.pending_config;
        }

        return (
            <MocapCameraControls config={config} camera_controls={group.camera_controls} onChange={(config) => {
                this.setState({ pending_config: config }, () => {
                    this._start_update();
                })
            }} />
        );
    }

    render() {
        let status = this.state.status;
        if (!status) {
            return <div>Loading...</div>;
        };

        let is_calibrating = status.calibration ? true : false;

        return (
            <div>
                <Title value="Mocap | Cameras" />
                <Navbar />

                <div className="container-fluid" style={{ paddingTop: 20, paddingBottom: 20 }}>
                    <div className="row">


                        {this._render_blob_views()}

                        <div className="col col-md-4">

                            <Card header="Status" style={{ marginBottom: 10 }}>
                                <div style={{ padding: '0 8px' }}>
                                    <table className="table table-hover" style={{ verticalAlign: "baseline", fontFamily: "Noto Sans Mono" }}>
                                        <thead>
                                            <tr>
                                                <th>Id</th>
                                                <th>PTP Error</th>
                                                <th>PPS Error</th>
                                                <th>Latency</th>
                                                <th>Power</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {(status.cameras || []).map((camera) => {
                                                return this._render_camera_status_row(camera);
                                            })}
                                        </tbody>
                                    </table>
                                </div>

                            </Card>


                            <Card header="Calibration" style={{ marginBottom: 10 }}>
                                <CardBody>
                                    <div className="btn-group me-2" role="group">

                                        <GroupButton
                                            active={is_calibrating}
                                            onClick={(done) => this._execute({ start_calibration: true }, done)}
                                        >
                                            Start
                                        </GroupButton>
                                        <GroupButton
                                            active={!is_calibrating}
                                            onClick={(done) => this._execute({ stop_calibration: true }, done)}
                                        >
                                            Stop
                                        </GroupButton>


                                    </div>
                                </CardBody>
                            </Card>


                            <Card header="Controls" style={{ marginBottom: 10 }}>
                                <CardBody>
                                    {this._render_controls()}
                                </CardBody>
                            </Card>
                        </div>

                    </div>

                </div>

            </div>
        );
    }
};


class GroupButton extends React.Component {
    render() {
        return (
            <Button disabled={this.props.disabled} preset={this.props.active ? "outline-dark" : "outline-secondary"} active={this.props.active} onClick={this.props.onClick}>{this.props.children}</Button>
        )
    }
}
