import React from "react";
import { PageContext } from "pkg/web/lib/page";
import { Card } from "../card";
import { VideoPlayer } from "pkg/web/lib/video";
import { VideoSourceKind } from "pkg/web/lib/video/types";
import { MachineUiState } from "./state";
import { VideoCrosshair } from "pkg/web/lib/video/crosshair";

// This box is used to render the camera live stream when it is detached from the main camera box. 
export class CameraLiveBox extends React.Component<{ machine: any, ui_state: MachineUiState, context: PageContext }> {
    render() {
        let ui_state = this.props.ui_state;

        if (!ui_state.camera_ui_state().enlarged_camera_id) {
            return null;
        }

        let machine = this.props.machine;

        let camera = null;

        let cameras = machine.config.cameras || [];
        cameras.map((c) => {
            if (c.id == ui_state.camera_ui_state().enlarged_camera_id) {
                camera = c;
            }
        });

        if (camera == null) {
            return null;
        }

        return (
            <Card id="live-view" header="Live Stream" style={{ marginBottom: 10 }}>
                <CameraLivePlayer camera={camera} machine={machine} context={this.props.context} ui_state={this.props.ui_state} />
            </Card>
        );
    }

}

export class CameraLivePlayer extends React.Component<{ machine: any, camera: any, context: PageContext, ui_state: MachineUiState }> {

    state = {
        _props: null
    }

    constructor(props) {
        super(props);
        this._get_properties();
    }

    async _get_properties() {
        try {
            let res = await this.props.context.channel.call('cnc.Monitor', 'GetCameraProperties', {
                machine_id: this.props.machine.id,
                camera_id: this.props.camera.id,
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }

            this.setState({ _props: res.responses[0] });

        } catch (e) {
            // TODO: Notification
            console.error(e);
        }

    }


    render() {
        let machine = this.props.machine;
        let camera = this.props.camera;

        if (!this.state._props) {
            return null;
        }

        let format = this.state._props.format.live_format;
        let kind = VideoSourceKind.Live;
        if (format == 'MJPEG') {
            kind = VideoSourceKind.MJPEG;
        }

        let state = this.props.ui_state.camera_ui_state();

        return (
            <div style={{ overflow: 'hidden' }}>
                <VideoPlayer source={{
                    kind: kind,
                    url: `/api/machines/${machine.id}/cameras/${camera.id}/stream`
                }}>
                    <VideoCrosshair size={state.crosshair_enabled ? state.crosshair_size : 0} />
                </VideoPlayer>
            </div>
        );

    }
};