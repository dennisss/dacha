import React from "react";
import { PageContext } from "../page";
import { Card, CardBody } from "../card";
import { PropertiesTable } from "../properties_table";
import { Button } from "pkg/web/lib/button";

export class ProgramRunFileBox extends React.Component<{ run: any, context: PageContext }> {

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

                    <Button preset="outline-primary" style={{ width: '100%' }} onClick={() => { }}>
                        Re-Load
                    </Button>
                </CardBody>
            </Card>
        );
    }
}