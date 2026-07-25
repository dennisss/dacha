import React from "react";
import { Router } from "pkg/web/lib/router";
import { PageContext } from "pkg/web/lib/page";
import { Title } from "pkg/web/lib/title";
import { Navbar, NAVBAR_HEIGHT } from "./navbar";
import { compare_values, deep_copy } from "pkg/web/lib/utils";
import { Button } from "pkg/web/lib/button";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";
import { round_digits, round_nested_digits } from "pkg/web/lib/formatting";
import { MocapCameraControls } from "../../camera/js/controls";
import { camera_orientation } from "../../camera/js/orientation";
import { entity_id_to_string } from "./utils";
import { PropertiesTable } from "pkg/cnc/monitor/js/properties_table";
import { DARK_MODE } from "pkg/web/lib/dark";
import { FrameViewer } from "./frame_viewer";

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
        pending_config: null,

        show_blobs: true,
        show_grayscale: false,
        show_fullscreen: false,
        show_crosshair: false,
    }

    constructor(props: CamerasPageProps) {
        super(props);
        this._get_status();
        this._read_blobs();
    }

    _get_status = async () => {
        if (this.props.context.channel.aborted()) {
            return;
        }

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

        // TODO: If we disable all cameras, this should ideally emit some empty frames to update the UI appropriately.
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

        // TODO: Ideally these all updated less frequently and maybe only contained P99 numbers.
        // We also want to make sure this detects one off flakiness rather than reporting the currnet state.
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

        let angle = '';
        if (camera.camera_status) {
            let o = camera_orientation(camera.camera_status);
            angle = <span>{o.horizon_angle}&deg;</span>;
        }

        let config = {};
        (this.state.status.config.per_camera || []).map((c) => {
            if (c.camera_id == camera.id) {
                config = c;
            }
        })

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
                    <input checked={config.enabled ? true : false} type="checkbox"
                        onChange={() => {
                            this._execute({
                                set_camera_enabled: {
                                    camera_id: camera.id,
                                    enabled: !config.enabled
                                }
                            }, () => { });
                        }}
                        onClick={(e) => {
                            e.stopPropagation();
                        }}
                    />
                </td>
                <td>
                    <a href={"https://" + entity_id_to_string(camera.id) + ".mocap_camera.worker.home.cluster.internal"}>
                        {entity_id_to_string(camera.id)}
                    </a>
                </td>
                <td>{color_format(ptp_error, ptp_age)}</td>
                <td>{color_format(pps_error, pps_age)}</td>
                <td>{latency}</td>
                <td>{angle}</td>
                <td><span className="material-symbols-fill">{config.intrinsics ? 'check' : 'error'}</span></td>
                <td><span className="material-symbols-fill">{config.extrinsics ? 'check' : 'error'}</span></td>
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
                configure_cameras: { config: data },
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

        let border_color = DARK_MODE.get() ? '#444' : '#ccc';

        let render_checkbox = (text, state_key, enabled) => {

            return (
                <div style={{ display: 'inline-block', padding: '5px 10px', cursor: (enabled ? 'pointer' : null) }} onClick={(e) => {
                    let s = {};
                    s[state_key] = !this.state[state_key];
                    this.setState(s);
                }}
                    onMouseDown={(e) => {
                        // Prevent text selection.
                        e.preventDefault();
                    }}
                >
                    <input type="checkbox" disabled={!enabled} onChange={() => { }} checked={this.state[state_key]} style={{ verticalAlign: -2 }} /> {text}
                </div>

            );
        }

        let active_camera = null;
        (status.cameras || []).map((c) => {
            if (c.active) {
                active_camera = c;
            }
        });


        let num_enabled = 0;
        (status.cameras || []).map((camera) => {
            let config = {};
            (status.config.per_camera || []).map((c) => {
                if (c.camera_id == camera.id) {
                    config = c;
                }
            });

            if (config.enabled) {
                num_enabled += 1;
            }
        });


        return (
            <div>
                <div style={{ fontFamily: "Noto Sans Mono", marginBottom: 10, fontSize: 12 }}>
                    {frame_text}

                    <div style={{ float: 'right' }}>
                        {render_checkbox('Blobs', 'show_blobs', true)}
                        {render_checkbox('Grayscale', 'show_grayscale', true)}
                        {render_checkbox('Fullscreen', 'show_fullscreen', true)}
                        {render_checkbox('Aim', 'show_crosshair', true)}
                    </div>
                </div>

                <div style={{ marginBottom: 10, marginLeft: -5, marginRight: -5 }}>
                    {(status.cameras || []).map((camera) => {
                        if (!status.groups) {
                            return null;
                        }

                        let config = {};
                        (status.config.per_camera || []).map((c) => {
                            if (c.camera_id == camera.id) {
                                config = c;
                            }
                        });

                        if (!config.enabled) {
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

                        let flip_x = false;
                        let flip_y = false;

                        if (camera.camera_status) {
                            let o = camera_orientation(camera.camera_status);
                            upside_down = o.upside_down;
                            angle = o.horizon_angle;

                            if (o.upside_down) {
                                flip_y = true;
                            } else {
                                flip_x = true;
                            }
                        }

                        let points = [];
                        if (blobs && this.state.show_blobs) {
                            points = blobs.results.blobs || [];
                        }


                        let url = null;
                        if (this.state.show_grayscale) {
                            url = "/camera?id=" + entity_id_to_string(camera.id);
                        }

                        // TODO: Integrate the loading opacity change into the FrameViewer since that has a separate one for image loading.
                        return (
                            <div key={camera.id}
                                style={{
                                    opacity: (blobs !== null ? 1 : 0.5),
                                    cursor: 'pointer',
                                    width: (active && this.state.show_fullscreen ? '100%' : (num_enabled < 5 ? '50%' : '33.333%')),
                                    display: (!active && active_camera && this.state.show_fullscreen ? 'none' : 'inline-block'),
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
                                    border: ('1px solid ' + (active ? '#08f' : border_color)),
                                    borderRadius: 5,
                                    overflow: 'hidden',
                                    boxShadow: (active ? '0px 0px 10px #08f' : '')
                                }}>
                                    <FrameViewer url={url} points={points} flipX={flip_x} flipY={flip_y} is_live_stream={true} controls_enabled={active} show_crosshair={this.state.show_crosshair} />
                                </div>
                            </div>
                        );
                    })}
                </div>

            </div>

        );


    }

    _render_blob_summary_card() {

        let num_blobs = 0;
        let radius_sum = 0;

        if (this.state.blobs) {
            (this.state.blobs.cameras || []).map((c) => {
                (c.results.blobs || []).map((b) => {
                    num_blobs += 1;
                    radius_sum += (b.radius_a + b.radius_b) / 2.0;
                })
            });

            radius_sum /= num_blobs;
        }


        return (
            <Card header="Blobs Summary" style={{ marginBottom: 10 }}>
                <div>
                    {this._render_object_table({
                        '# Blobs': num_blobs,
                        'Average Radius': radius_sum
                    })}
                </div>
            </Card>
        );
    }

    _render_blob_list() {
        let status = this.state.status;

        let active_camera = null;
        (status.cameras || []).map((c) => {
            if (c.active) {
                active_camera = c;
            }
        })

        if (!active_camera) {
            return this._render_blob_summary_card();
        }

        let rows = [];
        if (this.state.blobs) {
            let blobs = null;

            (this.state.blobs.cameras || []).map((c) => {
                if (c.camera_id == active_camera.id) {
                    blobs = c;
                }
            });

            if (blobs) {
                rows = (blobs.results.blobs || []).map((b, i) => {
                    return (
                        <tr key={i}>
                            <td>{round_digits(b.x, 2)}</td>
                            <td>{round_digits(b.y, 2)}</td>
                            <td>{round_digits(b.radius_a, 2)}</td>
                            <td>{round_digits(b.radius_b, 2)}</td>
                        </tr>
                    );
                })
            }
        }

        let header = (
            <div>
                Active Camera Blobs
                <span style={{ float: 'right' }}>({rows.length} total)</span>
            </div>
        );

        return (
            <Card header={header} style={{ marginBottom: 10 }}>
                <div style={{ padding: '0 8px' }}>
                    <table className="table table-hover" style={{ verticalAlign: "baseline", fontFamily: "Noto Sans Mono" }}>
                        <thead>
                            <tr>
                                <th style={{ width: '25%' }}>X</th>
                                <th style={{ width: '25%' }}>Y</th>
                                <th style={{ width: '25%' }}>Radius A</th>
                                <th style={{ width: '25%' }}>Radius B</th>
                            </tr>
                        </thead>
                        <tbody style={{ fontSize: '0.9em' }}>
                            {rows}
                        </tbody>
                    </table>
                </div>

            </Card>


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

    _render_intrinsics_card() {
        let status = this.state.status;

        let active_camera = null;
        (status.cameras || []).map((c) => {
            if (c.active) {
                active_camera = c;
            }
        })

        if (!active_camera) {
            return;
        }

        let config = {};
        (status.config.per_camera || []).map((c) => {
            if (c.camera_id == active_camera.id) {
                config = c;
            }
        })

        return (
            <Card header="Intrinsics" style={{ marginBottom: 10 }}>
                <div>
                    {config.intrinsics ? (
                        this._render_object_table(config.intrinsics)
                    ) : (
                        <CardBody>Not calibrated yet</CardBody>
                    )}
                </div>
                <CardBody>
                    <Button preset="outline-primary" onClick={(done) => {
                        this._execute({
                            start_checkerboard_calibration: { camera_id: active_camera.id }
                        }, done)
                    }}>
                        Start Checkerboard Calibration
                    </Button>

                </CardBody>
            </Card>
        )
    }

    _render_extrinsics_card() {
        let status = this.state.status;

        let active_camera = null;
        (status.cameras || []).map((c) => {
            if (c.active) {
                active_camera = c;
            }
        })

        if (!active_camera) {
            return;
        }

        let config = {};
        (status.config.per_camera || []).map((c) => {
            if (c.camera_id == active_camera.id) {
                config = c;
            }
        })

        return (
            <Card header="Extrinsics" style={{ marginBottom: 10 }}>
                <div>
                    {config.extrinsics ? (
                        this._render_object_table(config.extrinsics)
                    ) : (
                        <CardBody>Not calibrated yet</CardBody>
                    )}
                </div>
            </Card>
        )

    }

    _render_object_table(obj) {
        return (
            <div style={{ padding: '0 8px' }}>
                <table className="table" style={{ margin: 0 }}>
                    <tbody>
                        {Object.keys(obj).map((key) => {
                            return (
                                <tr key={key}>
                                    <td style={{ width: 1, whiteSpace: 'nowrap' }}>{key}</td>
                                    <td>{JSON.stringify(round_nested_digits(obj[key], 2))}</td>
                                </tr>
                            );
                        })}
                    </tbody>
                </table>
            </div>
        );
    }

    _render_wanding_card() {

        function make_button(text, disabled, on_click) {
            return (
                <div style={{ width: '25%', padding: '0 5px', display: 'inline-block' }}>
                    <Button preset='outline-primary' disabled={disabled} style={{ width: '100%', opacity: (disabled ? 0.5 : 1) }} onClick={on_click}>{text}</Button>
                </div>
            )
        }

        let status = this.state.status;

        let is_calibrating = false;

        let stats = null;
        let results = null;
        let have_results = false;
        let have_valid_results = false;

        if (status.mode && status.mode.wanding_calibration) {
            is_calibrating = true;

            let mode = status.mode.wanding_calibration;
            if (mode.stats) {
                let props = [];
                (Object.keys(mode.stats) || []).map((key) => {
                    props.push({
                        name: key,
                        value: JSON.stringify(round_nested_digits(mode.stats[key], 2))
                    })
                });

                stats = (
                    <div style={{ marginTop: 10, marginBottom: -10 }}>
                        <PropertiesTable properties={props} />
                    </div>
                );
            }

            if (mode.result) {
                have_results = true;

                have_valid_results = mode.result.failed ? false : true;

                let props = [];
                (Object.keys(mode.result) || []).map((key) => {
                    props.push({
                        name: key,
                        value: JSON.stringify(round_nested_digits(mode.result[key], 2))
                    })
                });

                results = (
                    <div style={{ marginTop: 40, marginBottom: -10 }}>
                        <div style={{ fontWeight: 'bold' }}>Results:</div>
                        <PropertiesTable properties={props} />
                    </div>
                );
            }
        }

        return (
            <Card header="Wanding Calibration" style={{ marginBottom: 10 }}>
                <CardBody>
                    <div style={{ margin: '0 -5px' }}>
                        {make_button('Start', is_calibrating, (done) => {
                            this._execute({ start_wanding_calibration: {} }, done)
                        })}
                        {make_button('Process', !is_calibrating || have_results, (done) => {
                            this._execute({ process_wanding_calibration: {} }, done)
                        })}
                        {make_button('Apply', !have_valid_results, (done) => {
                            this._execute({ apply_wanding_calibration: {} }, done)
                        })}
                        {make_button('Cancel', !is_calibrating, (done) => {
                            this._execute({ cancel_wanding_calibration: {} }, done)
                        })}
                    </div>
                    {stats}
                    {results}
                </CardBody>
            </Card>
        );
    }

    render() {
        let status = this.state.status;
        if (!status) {
            return <div>Loading...</div>;
        };

        // TODO: Do this at a higher level across all pages.
        if (status.mode && status.mode.checkerboard_calibration) {
            setTimeout(() => {
                Router.global().goto('/ui/checkerboard');
            });
            return <div></div>;
        }

        let status_header = (
            <div>
                <span>Status</span>

                <span style={{ float: 'right' }}>({(status.cameras || []).length} cameras)</span>
            </div>
        );


        let all_enabled = true;
        (status.cameras || []).map((camera) => {

            let config = {};
            (status.config.per_camera || []).map((c) => {
                if (c.camera_id == camera.id) {
                    config = c;
                }
            })

            if (!config.enabled) {
                all_enabled = false;
            }
        });


        return (
            <div>
                <Title value="Mocap | Cameras" />
                <Navbar />

                <div style={{ position: 'fixed', top: NAVBAR_HEIGHT, bottom: 0, right: 0, left: 0 }}>

                    <div style={{ width: '66.6667%', top: 0, bottom: 0, left: 0, position: 'absolute', overflow: 'scroll' }} className="noscrollbar">
                        <div style={{ padding: '20px 12px' }}>
                            {this._render_blob_views()}
                        </div>
                    </div>

                    <div style={{ width: '33.3333%', top: 0, bottom: 0, right: 0, position: 'absolute', overflow: 'scroll' }} className="noscrollbar">
                        <div style={{ padding: '20px 12px' }}>
                            <Card header={status_header} style={{ marginBottom: 10 }}>
                                <div style={{ padding: '0 8px' }}>
                                    <table className="table table-hover" style={{ verticalAlign: "baseline", fontFamily: "Noto Sans Mono" }}>
                                        <thead>
                                            <tr>
                                                <th>
                                                    <input type="checkbox" checked={all_enabled} onChange={() => {
                                                        // TODO: Have a fast timeout and error notice if this fails.
                                                        this._execute({
                                                            set_camera_enabled: {
                                                                all_cameras: true,
                                                                enabled: !all_enabled
                                                            }
                                                        }, () => { });
                                                    }} />
                                                </th>
                                                <th>Id</th>
                                                <th>PTP</th>
                                                <th>PPS</th>
                                                <th>Latency</th>
                                                <th>Angle</th>
                                                <th>Int</th>
                                                <th>Ext</th>
                                            </tr>
                                        </thead>
                                        <tbody style={{ fontSize: '0.9em' }}>
                                            {(status.cameras || []).map((camera) => {
                                                return this._render_camera_status_row(camera);
                                            })}
                                        </tbody>
                                    </table>
                                </div>

                            </Card>

                            {this._render_intrinsics_card()}

                            {this._render_extrinsics_card()}

                            {this._render_wanding_card()}

                            {/* TODO: Allow reading/writing just one camera's settings temporarily. */}
                            <Card header="Controls (all cameras)" style={{ marginBottom: 10 }}>
                                <CardBody>
                                    {this._render_controls()}
                                </CardBody>
                            </Card>

                            {/* NOTE: This should be last since it is fairable length and may shift the other stuff. */}
                            {this._render_blob_list()}
                        </div>
                    </div>
                </div>

            </div>
        );
    }
};

