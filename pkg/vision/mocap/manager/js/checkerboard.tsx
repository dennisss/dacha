import React from "react";
import { Router } from "pkg/web/lib/router";
import { PageContext } from "pkg/web/lib/page";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "./navbar";
import { compare_values, deep_copy } from "pkg/web/lib/utils";
import { Button } from "pkg/web/lib/button";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";
import { round_digits, round_nested_digits } from "pkg/web/lib/formatting";
import { MocapCameraControls } from "../../camera/js/controls";
import { camera_orientation } from "../../camera/js/orientation";
import { entity_id_to_string } from "./utils";
import { PropertiesTable } from "pkg/cnc/monitor/js/properties_table";
import { FrameViewer } from "./frame_viewer";


export interface CheckerboardPageProps {
    context: PageContext,
}

interface CheckerboardPageState {
    status: any,
    live: boolean,
    frame_index: any
}

export class CheckerboardPage extends React.Component<CheckerboardPageProps, CheckerboardPageState> {

    state = {
        status: null,
        live: true,
        frame_index: 0,
        pending_config: null,
    }

    constructor(props: CheckerboardPageProps) {
        // TODO: Need to leave this page if not currently calibrating.

        super(props);
        this._get_status();
        // this._read_blobs();
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
        let res = await this.props.context.channel.call('mocap.Manager', 'Status', {});
        if (!res.status.ok()) {
            throw res.status.toString();
        }

        this.setState({ status: res.responses[0] });
    }

    _render_controls(mode) {

        let status = this.state.status;
        if (!status.groups) {
            return;
        }

        let group = status.groups[0];

        if (!group.config || !group.camera_controls) {
            return null;
        }

        let config = group.config;
        if (status.single_camera_override && status.single_camera_override.camera_id == mode.camera_id) {
            config = status.single_camera_override.config;
        }

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

    // TODO: Dedup this everywhere.
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
            let mode = this.state.status.mode.checkerboard_calibration;

            let res = await this.props.context.channel.call('mocap.Manager', 'Execute', {
                configure_cameras: { camera_id: mode.camera_id, config: data },
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

    _execute = async (req, done) => {

        try {
            let res = await this.props.context.channel.call('mocap.Manager', 'Execute', req);
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            await this._get_status_once();

        } finally {
            done()
        }
    }

    _render_overview(mode) {

        function make_button(text, preset, on_click) {
            return (
                <div style={{ width: '25%', padding: '0 5px', display: 'inline-block' }}>
                    <Button preset={preset} style={{ width: '100%' }} onClick={on_click}>{text}</Button>
                </div>
            )
        }

        let num_frames = (mode.frames || []).length;

        let num_good_frames = 0;
        (mode.frames || []).map((frame) => {
            if ((frame.points_2d || []).length > 0) {
                num_good_frames += 1;
            }
        });

        let properties = [
            {
                name: 'Camera Id',
                value: entity_id_to_string(mode.camera_id),
            },
            {
                name: 'Run Id',
                value: mode.run_id
            },
            {
                name: 'Num Frames',
                value: `${num_good_frames} / ${num_frames}`
            }
        ];


        let result = null;

        if (mode.result) {
            let props = [
                {
                    name: 'Error (RMS)',
                    value: round_digits(mode.result.error, 2)
                }
            ];

            (Object.keys(mode.result.intrinsics) || []).map((key) => {
                props.push({
                    name: key,
                    value: JSON.stringify(round_nested_digits(mode.result.intrinsics[key], 2))
                })
            });

            result = (
                <div style={{ marginTop: 40, marginBottom: -10 }}>
                    <div style={{ fontWeight: 'bold' }}>Results:</div>
                    <PropertiesTable properties={props} />
                </div>
            );

        }

        return (
            <Card header="Overview" style={{ marginBottom: 10 }}>
                <CardBody>
                    <div style={{ margin: '0 -5px' }}>
                        {make_button('Capture', 'outline-primary', (done) => {
                            this._execute({ capture_checkerboard_frame: true }, () => {
                                // TODO: only do on success.
                                let num_frames = (this.state.status.mode.checkerboard_calibration.frames || []).length;
                                this.setState({ live: false, frame_index: num_frames - 1 });

                                done();
                            })
                        })}
                        {make_button('Process', 'outline-primary', (done) => {
                            this._execute({ process_checkerboard_calibration: {} }, done)
                        })}
                        {make_button('Apply', 'outline-primary', (done) => {
                            this._execute({ apply_checkerboard_calibration: {} }, done)
                        })}
                        {make_button('Cancel', 'outline-primary', (done) => {
                            this._execute({ cancel_checkerboard_calibration: {} }, done)
                        })}
                    </div>
                    <div style={{ marginTop: 10, marginBottom: -10 }}>
                        <PropertiesTable properties={properties} />
                    </div>
                    {result}
                </CardBody>


            </Card>

        );

    }

    _render_frame_viewer(mode) {

        let camera = null;
        this.state.status.cameras.map((c) => {
            if (c.id == mode.camera_id) {
                camera = c;
            }
        });

        let flip_x = false;
        let flip_y = false;
        if (camera.camera_status) {
            let o = camera_orientation(camera.camera_status);

            if (o.upside_down) {
                flip_y = true;
            } else {
                flip_x = true;
            }
        }

        let num_frames = (mode.frames || []).length;

        let image_source = { camera_id: mode.camera_id };
        let points = [];
        if (!this.state.live && this.state.frame_index < num_frames) {
            image_source = { url: mode.frames[this.state.frame_index].image_path };

            points = (mode.frames[this.state.frame_index].points_2d || []).map((pt) => {
                return {
                    x: pt.values[0],
                    y: pt.values[1],
                    radius_a: 10,
                    radius_b: 10,
                    angle: 0,
                };
            });
        }

        return (
            <FrameViewer image_source={image_source} points={points} flipX={flip_x} flipY={flip_y} channel={this.props.context.channel} />
        );
    }

    _render_frame_viewer_controls(mode) {

        let num_frames = (mode.frames || []).length;


        return (
            <Card header="Frame Viewer" style={{ marginBottom: 10 }}>
                <CardBody>
                    <table className="table" style={{ cursor: 'pointer', margin: 0 }}>
                        <tbody>
                            <tr onClick={() => this.setState({ live: true })}>
                                <td style={{ width: 1 }}>
                                    <input type="radio" checked={this.state.live} readOnly />
                                </td>
                                <td style={{ width: 1 }}>
                                    Live
                                </td>
                                <td></td>
                                <td></td>

                            </tr>
                            <tr onClick={() => this.setState({ live: false })}>
                                <td style={{ width: 1 }}>
                                    <input type="radio" checked={!this.state.live} readOnly />
                                </td>
                                <td style={{ width: 1 }}>Captured</td>
                                <td>
                                    <input style={{ width: '100%' }} type="range" value={this.state.frame_index} min={0} max={num_frames - 1} value={this.state.frame_index} onChange={(e) => {
                                        this.setState({
                                            frame_index: e.target.value * 1
                                        })
                                    }} />
                                </td>
                                <td>{this.state.frame_index}</td>
                            </tr>
                        </tbody>
                    </table>
                </CardBody>
            </Card>
        );
    }

    render() {
        let status = this.state.status;
        if (!status) {
            return <div></div>;
        };

        if (!status.mode || !status.mode.checkerboard_calibration) {
            setTimeout(() => {
                Router.global().goto("/");
            });
            return <div></div>;
        }

        let mode = status.mode.checkerboard_calibration;

        return (
            <div>
                <Title value="Mocap | Checkerboard Calibration" />
                <Navbar />

                <div className="container-fluid" style={{ paddingTop: 20, paddingBottom: 20 }}>
                    <div className="row">
                        <div className="col col-md-8">
                            <div style={{ width: '100%', border: '1px solid #444' }}>
                                {this._render_frame_viewer(mode)}
                            </div>
                        </div>

                        <div className="col col-md-4">
                            {this._render_overview(mode)}

                            {this._render_frame_viewer_controls(mode)}

                            <Card header="Camera Controls" style={{ marginBottom: 10 }}>

                                <CardBody>
                                    {this._render_controls(mode)}
                                </CardBody>
                            </Card>
                        </div>

                    </div>

                </div>

            </div>
        );
    }
};

