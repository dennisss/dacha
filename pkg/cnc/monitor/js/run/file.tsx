import React from "react";
import { PageContext } from "../page";
import { Card, CardBody } from "../card";
import { PropertiesTable } from "../properties_table";
import { Button } from "pkg/web/lib/button";
import { pick_machine } from "../machine_picker";
import { Router } from "pkg/web/lib/router";

export class ProgramRunFileBox extends React.Component<{ run: any, context: PageContext }> {

    // TODO: Dedup this code.
    _on_click_load = async (done: any) => {
        done();

        let ctx = this.props.context;

        try {
            let machine_id = await pick_machine(ctx.channel, this.props.run.machine_id);

            let res = await ctx.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: machine_id,
                load_program: {
                    file_id: this.props.run.file_id
                }
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }

            Router.global().goto('/ui/machines/' + machine_id);

        } catch (e) {
            console.error(e);
            // TODO: Make notification
        }
    }

    render() {
        let run = this.props.run;

        if (!run.file) {
            return null;
        }


        let properties = []

        if (run.file.has_thumbnail) {
            // TODO: Dedup the thumbnail image bytes retrieval logic.
            properties.push({
                name: 'Thumbnail:',
                value: (
                    <img src={run.file.urls.thumbnail_url} style={{ width: '100%' }} />
                )
            })
        }

        properties.push(
            {
                name: 'Id:',
                value: run.file_id
            },
            {
                name: 'Name:',
                value: run.file.name
            },
        )

        return (
            <Card id="run-file" header="File" style={{ marginBottom: 10 }}>
                <CardBody>
                    <PropertiesTable properties={properties} />

                    <Button preset="outline-primary" style={{ width: '100%' }} onClick={this._on_click_load}>
                        Re-Load
                    </Button>
                </CardBody>
            </Card>
        );
    }
}