import React from "react";
import { PageContext } from "../page";
import { Card, CardBody } from "../card";
import { timestamp_proto_to_millis } from "pkg/web/lib/formatting";
import { FragmentedVideoSourceOptions, MediaFragment, MediaSegmentData, VideoSourceKind, VideoSourceOptions } from "pkg/web/lib/video/types";
import { VideoPlayer } from "pkg/web/lib/video";


export class ProgramRunVideoBox extends React.Component<{ run: any, context: PageContext, machine: any }, { _source: VideoSourceOptions | null }> {

    state = {
        _source: null
    }

    constructor(props) {
        super(props);

        this._load();
    }

    async _load() {
        let ctx = this.props.context;
        let machine = this.props.machine;
        let run = this.props.run;

        let cameras = machine.config.cameras || [];
        if (cameras.length == 0) {
            return;
        }

        // Both in microseconds.
        let start_time = Math.round(timestamp_proto_to_millis(run.start_time) * 1000);
        let end_time = Math.round(timestamp_proto_to_millis(run.end_time ? run.end_time : run.last_updated) * 1000);

        let res = await ctx.channel.call('cnc.Monitor', 'GetCameraPlayback', {
            machine_id: machine.id,
            camera_id: cameras[0].id,
            start_time,
            end_time,
        });

        if (!res.status.ok()) {
            throw res.status.toString();
        }

        let msg = res.responses[0];

        function convert_segment_data(data: any): MediaSegmentData {
            return {
                segment_url: data.segment_url,
                byte_range: (data.byte_range ? {
                    start: (data.byte_range.start || 0) * 1,
                    end: (data.byte_range.end || 0) * 1,
                } : undefined)
            }
        }

        if ((msg.fragments || []).length == 0) {
            // No data so can't display a video.
            return;
        }

        let source: FragmentedVideoSourceOptions = {
            kind: VideoSourceKind.Fragmented,
            start_time: start_time / 1000000,
            end_time: end_time / 1000000,
            fragments: (msg.fragments || []).map((f) => {
                let out: MediaFragment = {
                    start_time: (f.start_time || 0) / 1000000,
                    end_time: (f.end_time || 0) / 1000000,
                    relative_start: (f.relative_time || 0) / 1000000,
                    data: convert_segment_data(f.data),
                    init_data: f.init_data ? convert_segment_data(f.init_data) : undefined,
                    mime_type: f.mime_type
                };

                return out;
            })
        };

        this.setState({ _source: source });
    }

    render() {
        if (!this.state._source) {
            return null;
        }

        let run = this.props.run;

        // TODO: Have a drop down to pick between different cameras.

        // TODO: Need to render which time segments are available to seek to (some ranges may be missing data / paused)

        // TODO: The overflow hidden may mess up tooltips added in the future.

        return (
            <Card id="run-video" header="Camera Playback" style={{ marginBottom: 10, overflow: 'hidden' }}>
                <VideoPlayer source={this.state._source} />
            </Card>
        );
    }
}
