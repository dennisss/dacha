import React from "react";
import { PageContext } from "../page";
import { Card, CardBody } from "../card";
import { PropertiesTable } from "../properties_table";
import { format_timecode_secs, timestamp_proto_to_millis } from "pkg/web/lib/formatting";

// Compared two times in second units
function approx_equal(a: number, b: number): boolean {
    return Math.abs(a - b) < 1;
}

export class ProgramRunTimelineBox extends React.Component<{ run: any, context: PageContext }> {
    /*
    TODOs:
    - Show a seek-bar / movie-editor style timeline with:
        - Red vertical bar at current time in video.
        - Gray bars at each layer/height start.
        - Separate tracks for each tool / color / camera being used.

    - Below the seek-bar show a raw flat list of events that happened in each segment (tool changes, layer starts, etc.) 
    */

    render() {
        let run = this.props.run;

        // All times in this function are in seconds.
        let overall_start_time = timestamp_proto_to_millis(run.start_time) / 1000;
        let overall_end_time = timestamp_proto_to_millis(run.end_time ? run.end_time : run.last_updated) / 1000;

        // console.log(overall_start_time, overall_end_time);

        let rows = [];

        // NOTE: The segments should be pre-sorted.
        let last_time = overall_start_time;
        let segments = run.playing_segments || [];
        segments.map((segment) => {
            let start_time = timestamp_proto_to_millis(segment.start_time) / 1000;
            let end_time = segment.end_time ? timestamp_proto_to_millis(segment.end_time) / 1000 : null;

            if (!approx_equal(start_time, last_time)) {
                rows.push({
                    line: '',
                    start: last_time,
                    end: start_time,
                    type: 'PAUSED'
                });
            }

            rows.push({
                line: segment.start_line || 0,
                start: start_time,
                end: end_time,
                type: 'PLAYING'
            });

            // NOTE: The last time may be null, but we will never do any comparisons on it.
            last_time = end_time;
        });


        return (
            <Card id="run-timeline" header="Timeline" style={{ marginBottom: 10 }}>
                <CardBody>
                    <table className="table">
                        <thead>
                            <tr>
                                <th>Line</th>
                                <th>Start</th>
                                <th>End</th>
                                <th>Type</th>
                            </tr>
                        </thead>
                        <tbody>
                            {rows.map((row, i) => {
                                return (
                                    <tr key={i}>
                                        <td style={{ whiteSpace: 'nowrap', width: 1 }}>
                                            {row.line}
                                        </td>
                                        <td style={{ whiteSpace: 'nowrap', width: 1 }}>
                                            {format_timecode_secs(
                                                row.start - overall_start_time,
                                                overall_end_time - overall_start_time)}
                                        </td>
                                        <td style={{ whiteSpace: 'nowrap', width: 1 }}>
                                            {row.end ? format_timecode_secs(
                                                row.end - overall_start_time,
                                                overall_end_time - overall_start_time) : ''}
                                        </td>
                                        <td>
                                            {row.type}
                                        </td>
                                    </tr>
                                );
                            })}
                        </tbody>
                    </table>
                </CardBody>
            </Card>
        );
    }
}