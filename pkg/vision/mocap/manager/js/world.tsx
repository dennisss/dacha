import React from "react";
import { PageContext } from "pkg/web/lib/page";
import { Title } from "pkg/web/lib/title";
import { Navbar, NAVBAR_HEIGHT } from "./navbar";
import { MocapWorldViewer } from "./world_viewer";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";
import { Button } from "pkg/web/lib/button";
import { center_points } from "./utils";
import { round_nested_digits } from "pkg/web/lib/formatting";
import { Setting } from "pkg/web/lib/settings";
import { PropertiesTable } from "pkg/cnc/monitor/js/properties_table";
import { DARK_MODE } from "pkg/web/lib/dark";

// TODO: Continously fetch the manager status across all pages.

export interface WorldPageProps {
    context: PageContext,
}

interface WorldPageState {
    status: any,
    points: any,
}

const CONTROLS = [
    {
        name: 'Dark Mode',
        setting: DARK_MODE,
        viewer_fn: (viewer: MocapWorldViewer, value: boolean) => viewer.setDarkMode(value)
    },
    {
        name: 'Show Cameras',
        setting: new Setting('world.show_cameras', true),
        viewer_fn: (viewer: MocapWorldViewer, value: boolean) => viewer.setCamerasVisible(value)
    },
    {
        name: 'Show Axis Labels',
        setting: new Setting('world.show_axis_labels', false),
        viewer_fn: (viewer: MocapWorldViewer, value: boolean) => viewer.setLabelsVisible(value)
    },
    {
        name: 'Show Rigid Bodies',
        setting: new Setting('world.show_rigid_bodies', true),
        viewer_fn: (viewer: MocapWorldViewer, value: boolean) => viewer.setRigidBodiesVisible(value)
    },
    {
        name: 'Show Rigid Body Axes',
        setting: new Setting('world.show_rigid_body_axis', false),
        viewer_fn: (viewer: MocapWorldViewer, value: boolean) => viewer.setRigidBodyAxesVisible(value)
    }
];

export class WorldPage extends React.Component<WorldPageProps, WorldPageState> {

    state = {
        status: null,
        points: null,
        selected_ids: [],
        menu_open: true,
    }

    _viewer: MocapWorldViewer;
    _canvas_container: React.RefObject<HTMLDivElement> = React.createRef();
    _selection_box: React.RefObject<HTMLDivElement> = React.createRef();

    constructor(props: WorldPageProps) {
        super(props);
    }

    componentDidMount(): void {
        this._viewer = new MocapWorldViewer(this._canvas_container.current, this._selection_box.current);
        this._update_viewer_controls();
        this._viewer.start();

        // TODO: Also detect a camera being selected and make it active/unactive.

        this._viewer.onSelectionChanged = (ids) => {
            this.setState({ selected_ids: ids })
        };

        this._get_status();
        this._read_points();
    }

    componentWillUnmount(): void {
        this._viewer.stop();
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

        this.setState({ status: res.responses[0] }, () => this._update_viewer());
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

    _read_points = async () => {

        // TODO: Retry everything with exponential backoff.

        let res = this.props.context.channel.call_streaming('mocap.MocapManager', 'ReadTrackedPoints', {
            max_rate: 10
        });
        // TODO: Check the response status.

        while (true) {
            let msg = await res.recv();
            if (!msg) {
                // TODO: This is an error
                return;
            }

            this.setState({ points: msg }, () => this._update_viewer());
        }
    }

    _update_viewer() {

        if (!this.state.status || !this.state.points) {
            return;
        }

        let points = [];
        if (this.state.points) {
            points = this.state.points.points;
        }

        let cameras_data = [];

        (this.state.status.cameras || []).map((cam) => {
            // TODO: Exclude 

            let config = {};
            (this.state.status.config.per_camera || []).map((c) => {
                if (c.camera_id == cam.id) {
                    config = c;
                }
            });

            if (!config.extrinsics) {
                return;
            }

            cameras_data.push({
                id: cam.id,
                translation: config.extrinsics.translation,
                rotation: config.extrinsics.rotation,
            });
        });

        let rigid_data = [];

        (this.state.status.config.rigid_body_tracker.bodies || []).map((body) => {
            let body_state = {};
            (this.state.points.rigid_bodies || []).map((s) => {
                if (s.id == body.id) {
                    body_state = s;
                }
            });

            if (!body_state || !body_state.found) {
                return;
            }

            rigid_data.push({
                id: body.id,
                points: body.points,
                // point_ids: body_state.point_ids,
                translation: body_state.translation,
                rotation: body_state.rotation,
            })
        });

        let data = {
            cameras: cameras_data,
            points,
            rigid_bodies: rigid_data
        };

        this._viewer.update(data);
    }

    _update_viewer_controls() {
        CONTROLS.map((c) => {
            c.viewer_fn(this._viewer, c.setting.get());
        });
    }

    _define_rigid_body = async (done) => {
        let config = this.state.status.config;

        try {
            let id = 1;
            (config.rigid_body_tracker.bodies || []).map((body) => {
                id = Math.max(body.id * 1 + 1, id);
            });

            let points = [];

            this.state.points.points.map((pt) => {
                if (this.state.selected_ids.indexOf(pt.id) >= 0) {
                    points.push(pt.position);
                }
            })

            points = center_points(points);

            await this._execute({ configure_rigid_body: { id, points } }, () => { })

        } catch (e) {
            console.error(e);
        }


        done();
    }

    _delete_rigid_body = (id, done) => {
        this._execute({ delete_rigid_body: id }, done)
    }

    _render_controls() {

        let props = CONTROLS.map((c) => {
            return {
                name: c.name,
                value: (
                    <input type="checkbox" checked={c.setting.get()} onChange={(e) => {
                        let v = e.target.checked;
                        c.setting.set(v);
                        this._update_viewer_controls();
                        this.forceUpdate();
                    }} />
                )
            };
        })

        return (
            <Card header="UI Controls" style={{ marginBottom: 10 }}>
                <div style={{ padding: '0 8px' }}>
                    <PropertiesTable properties={props} />
                </div>
            </Card>
        );

    }

    _render_menu() {
        // TODO: Not enough if the points are already in another rigid body.
        let have_enough_points = this.state.selected_ids.length >= 4;

        let border_color = DARK_MODE.get() ? '#444' : '#ccc';

        // TODO: If some points are selected, display info about them.
        // (selected points table and distance if just two)

        return (
            <div style={{ width: '30%', top: 0, bottom: 0, right: 0, position: 'absolute', overflow: 'scroll', borderLeft: `1px solid ${border_color}` }} className="noscrollbar">
                <div style={{ padding: '20px 12px' }}>
                    {this._render_controls()}

                    <Card header="Rigid Bodies" style={{ marginBottom: 10 }}>
                        {this._render_rigid_body_list()}
                        <CardBody>
                            <Button onClick={this._define_rigid_body} preset="outline-primary" disabled={!have_enough_points}>
                                {have_enough_points ? 'Create rigid body' : 'Select points to make a new body'}</Button>
                        </CardBody>
                    </Card>

                    <Card header="Calibration" style={{ marginBottom: 10 }}>
                        <CardBody>
                            <Button onClick={(done) => this._execute({ set_origin: {} }, done)} preset="outline-primary">
                                Set Origin
                            </Button>
                        </CardBody>
                    </Card>

                </div>
            </div>
        );

    }

    _render_rigid_body_list() {


        if (!this.state.status || !this.state.points) {
            return <div></div>;
        }

        let config = this.state.status.config;
        let res = this.state.points;

        // TODO: Clicking on a body or selecting one of the points in it should highlight it.

        return (
            <div style={{ padding: '0 8px' }}>
                <table className="table table-hover" style={{ verticalAlign: "baseline", fontFamily: "Noto Sans Mono", marginBottom: 0 }}>
                    <thead>
                        <tr>
                            <th>Id</th>
                            <th>Position</th>
                            <th></th>
                        </tr>
                    </thead>
                    <tbody style={{ fontSize: '0.9em' }}>
                        {(config.rigid_body_tracker.bodies || []).map((body) => {
                            let body_state = {};
                            (res.rigid_bodies || []).map((s) => {
                                if (s.id == body.id) {
                                    body_state = s;
                                }
                            });

                            let active = false;
                            (body_state.point_ids || []).map((id) => {
                                if (this.state.selected_ids.indexOf(id) >= 0) {
                                    active = true;
                                }
                            });

                            return (
                                <tr key={body.id} className={active ? 'table-active' : ''}>
                                    <td style={{ width: 1 }}>{body.id}</td>
                                    <td>{body_state.found ? JSON.stringify(round_nested_digits(body_state.translation.values, 4)) : '?'}</td>
                                    <td style={{ width: 1 }}>
                                        <Button preset={DARK_MODE.get() ? 'outline-light' : "outline-dark"} onClick={(done) => this._delete_rigid_body(body.id, done)} style={{ lineHeight: 1 }}>
                                            <span className="material-symbols-outlined">delete</span>
                                        </Button>
                                    </td>
                                </tr>
                            )
                        })}
                    </tbody>
                </table>
            </div>
        );

    }

    _toggle_menu = () => {
        let menu_open = this.state.menu_open;
        this.setState({ menu_open: !menu_open }, () => {
            this._viewer.onResize()
        });
    }

    render() {
        let menu_open = this.state.menu_open;

        return (
            <div>
                <Title value="Mocap | World" />
                <Navbar
                    togglerActive={menu_open}
                    togglerClick={this._toggle_menu}
                />

                <div style={{ position: 'fixed', top: NAVBAR_HEIGHT, bottom: 0, right: 0, left: 0 }}>

                    <div style={{ width: (menu_open ? '70%' : '100%'), top: 0, bottom: 0, left: 0, position: 'absolute', overflow: 'scroll' }} className="noscrollbar">

                        <div ref={this._canvas_container} style={{ width: '100%', height: '100%', position: 'absolute', top: 0, left: 0 }}></div>
                        <div ref={this._selection_box} className="selection-box"></div>
                    </div>

                    {menu_open ? this._render_menu() : null}

                </div>
            </div>
        );
    }
};
