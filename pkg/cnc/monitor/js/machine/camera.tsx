import React from "react";
import { PageContext } from "../page";
import { DeviceSelectorInput } from "../device_selector";
import { run_machine_command } from "../rpc_utils";
import { VideoPlayer } from "pkg/media/web/js/player";
import { PropertiesTable } from "../properties_table";
import { Button } from "pkg/web/lib/button";
import { deep_copy } from "pkg/web/lib/utils";
import { CardError } from "../card_error";
import { Card } from "../card";


export class CamerasBox extends React.Component<{ machine: any, context: PageContext }> {

    state = {
        _active_page: null
    }

    render() {
        let machine = this.props.machine;

        let cameras = machine.config.cameras || [];
        let camera_states = machine.state.cameras || [];

        return (
            <div>
                {cameras.length == 0 ? (
                    <CameraBox context={this.props.context} machine={machine} camera={null} camera_state={null} />
                ) : null}
                {cameras.map((camera) => {
                    let camera_state = null;
                    for (var i = 0; i < camera_states.length; i++) {
                        if (camera_states[i].camera_id == camera.id) {
                            camera_state = camera_states[i];
                            break;
                        }
                    }

                    return (
                        <CameraBox key={camera.id} context={this.props.context}
                            machine={machine} camera={camera} camera_state={camera_state} />
                    );
                })}
            </div>

        );

    }
};

interface CameraBoxProps {
    machine: any

    context: PageContext

    // This two may be null if we are creating a new camera.
    camera: any
    camera_state: any
}

interface CameraBoxState {
    _active_page: string | null,

    // Config proto for the camera. Defaults to this.props.camera
    _config: any

    // Will be none if config.device hasn't been edited.
    _selected_device: any | null;

    // If true, _config contains custom values that are out of sync with 
    _editing: boolean
}

class CameraBox extends React.Component<CameraBoxProps, CameraBoxState> {

    constructor(props: CameraBoxProps) {
        super(props)

        this.state = {
            _active_page: null,
            _config: deep_copy(props.camera || {
                record_while_playing: true,
                record_while_paused: false
            }),
            _editing: props.camera ? false : true
        };
    }

    componentDidUpdate() {
        if (!this.state._editing) {
            if (JSON.stringify(this.state._config) != JSON.stringify(this.props.camera)) {
                this.setState({
                    _config: deep_copy(this.props.camera)
                });
            }
        }
    }

    // TODO: If we have multiple cameras, we must lock change ops since we currently can't separately mutate different cameras.

    _pick_camera_device = (selector, done) => {

        let config = deep_copy(this.state._config);
        config.device = selector;

        console.log(config);

        run_machine_command(this.props.context, this.props.machine, {
            update_config: {
                cameras: [config],
                clear_fields: [{ key: [{ field_name: 'cameras' }] }]
            }
        }, done);
    }

    _click_revert = (done) => {
        this.setState({
            _editing: false,
            _config: deep_copy(this.props.camera),
            _selected_device: null,
        });
        done();
    }

    _click_save = (done) => {
        this._pick_camera_device(this.state._config.device, () => {
            // TODO: Only revert if successful.
            this._click_revert(done);
        });
    }

    _click_disconnect = (done) => {
        this._pick_camera_device(null, done);
    }

    render() {
        let machine = this.props.machine;
        let camera = this.state._config; // Will always be non-null.
        let camera_state = this.props.camera_state;

        let page = this.state._active_page == null ? (camera_state ? 'LIVE' : 'SETTINGS') : this.state._active_page;

        let live_page_allowed = camera_state && camera_state.status != 'MISSING';

        if (page == 'LIVE' && !live_page_allowed) {
            page = 'SETTINGS';
            setTimeout(() => {
                this.setState({ _active_page: page });
            });
        }

        // TODO: Must verify the camera is in a non-missing state before trying to open it.

        let settings_properties = [
            {
                name: 'Record While Playing',
                value: (
                    <input className="form-check-input" type="checkbox"
                        checked={camera.record_while_playing || false} onChange={(e) => {
                            let checked = e.target.checked;

                            let config = deep_copy(camera);
                            config.record_while_playing = checked;
                            this.setState({ _config: config, _editing: true });
                        }} />
                )
            },
            {
                name: 'Record While Paused',
                value: (
                    <input className="form-check-input" type="checkbox"
                        checked={camera.record_while_paused || false} onChange={(e) => {
                            let checked = e.target.checked;

                            let config = deep_copy(camera);
                            config.record_while_paused = checked;
                            this.setState({ _config: config, _editing: true });
                        }} />
                )
            }
        ]

        if (camera) {
            settings_properties.splice(0, 0, {
                name: 'Id',
                value: camera.id
            });
        }

        return (
            <Card
                id="camera"
                style={{ marginBottom: 10, overflow: 'hidden' }}
                header={(
                    <>
                        Camera

                        {camera_state ? (
                            <div style={{ float: 'right' }}>
                                <span className={"badge rounded-pill bg-" + (camera_state.status == 'RECORDING' ? 'danger' : 'secondary')}>
                                    {camera_state.status}
                                </span>
                            </div>

                        ) : null}
                    </>
                )}
                error={camera_state ? camera_state.last_error : null}
            >

                {page == 'LIVE' ? (
                    <div>
                        <VideoPlayer src={`/api/machines/${machine.id}/cameras/${camera.id}/stream`} />
                    </div>
                ) : null}

                {page == 'SETTINGS' ? (

                    <div className="card-body" style={{ borderBottom: '1px solid rgba(0,0,0,.125)', }}>
                        <div style={{ fontWeight: 'bold', paddingBottom: 5 }}>
                            General
                        </div>

                        <PropertiesTable style={{ fontSize: '0.8em' }} properties={settings_properties} />

                        <div style={{ paddingTop: 10 }}>
                            <div style={{ fontWeight: 'bold', paddingBottom: 5 }}>
                                Device Selector
                            </div>

                            <DeviceSelectorInput
                                context={this.props.context}
                                filter={(device) => device.video_path ? true : false}
                                selector={camera.device}
                                device={this.state._selected_device || (camera_state ? camera_state.device : null)}
                                connected_device={camera_state ? camera_state.device : null}
                                onChange={(selector, device) => {
                                    let config = deep_copy(camera);
                                    config.device = selector;
                                    this.setState({ _config: config, _selected_device: device, _editing: true });
                                }}
                            />
                        </div>

                        {camera_state ? (
                            (this.state._editing ? (
                                <div style={{ display: 'flex' }}>
                                    <Button style={{ flexGrow: 1, marginRight: 5 }} preset="primary" onClick={this._click_save}>
                                        Save
                                    </Button>
                                    <Button style={{ flexGrow: 1, marginLeft: 5 }} preset="dark" onClick={this._click_revert}>
                                        Revert
                                    </Button>
                                </div>
                            ) : null)


                        ) : (camera.device ? (
                            <Button preset="outline-primary" style={{ width: '100%' }} onClick={this._click_save}>
                                Connect
                            </Button>
                        ) : null)}

                        {camera_state ? (
                            <div style={{ paddingTop: 20 }}>
                                <Button preset="danger" onClick={(done) => { }}
                                    style={{ width: '100%' }}>
                                    Delete camera and all data
                                </Button>
                            </div>

                        ) : null}
                    </div>

                ) : null}

                <div style={{ fontSize: '0.8em' }}>
                    <ul className="nav nav-pills">
                        <NavItem active={page == 'LIVE'} disabled={!live_page_allowed} onClick={() => this.setState({ _active_page: 'LIVE' })}>
                            Live View
                        </NavItem>
                        <NavItem active={page == 'SETTINGS'} onClick={() => this.setState({ _active_page: 'SETTINGS' })}>
                            Settings
                        </NavItem>
                    </ul>
                </div>
            </Card>
        );
    }

}

class NavItem extends React.Component<{ disabled?: boolean, active: boolean, onClick: () => void }> {
    render() {
        return (
            <li className="nav-item" onClick={(e) => {
                e.preventDefault();

                if (this.props.disabled) {
                    return;
                }

                this.props.onClick();
            }}>
                <a className={"nav-link" + (this.props.active ? ' active' : '') + (this.props.disabled ? ' disabled' : '')}
                    style={{ borderRadius: 0 }} href="#">
                    {this.props.children}
                </a>
            </li>
        );

    }
}

