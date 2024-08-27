import React from "react";
import { PageContext } from "../page";
import { Card, CardBody } from "../card";
import { PropertiesTable } from "../properties_table";
import { Button } from "pkg/web/lib/button";
import { get_program_run_properties } from "../machine/player";
import { timestamp_proto_to_millis } from "pkg/web/lib/formatting";

export class ProgramRunStatsBox extends React.Component<{ run: any, context: PageContext }> {

    render() {
        let run = this.props.run;
        let properties = get_program_run_properties(run, run.file);

        if (run.start_time) {
            properties.push({
                name: 'Start Time:',
                value: new Date(timestamp_proto_to_millis(run.start_time)).toLocaleString()
            });
        }


        let end_time = null;
        let end_timeout = false;
        if (run.end_time) {
            end_time = new Date(timestamp_proto_to_millis(run.end_time));
        } else if (run.last_updated) {
            end_time = new Date(timestamp_proto_to_millis(run.last_updated));

            // 'Timing out' means that most likely the program/server crashed before being gracefully stopped.
            if (new Date().getTime() - end_time.getTime() > 90 * 1000) {
                end_timeout = true;
            }
        }


        if (end_time) {
            properties.push({
                name: 'End Time:',
                value: end_time.toLocaleString() + (end_timeout ? ' (Timeout)' : '')
            });
        }
        // TODO: 

        return (
            <Card id="run-stats" header="Stats" style={{ marginBottom: 10 }}>
                <CardBody>
                    <PropertiesTable properties={properties} />
                </CardBody>
            </Card>
        );
    }
}