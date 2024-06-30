

/*
Need to be able to:

- Change name

- Delete the machine.
    - Will need a confirmation

*/

import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { Figure } from "pkg/web/lib/figure";
import { round_digits } from "pkg/web/lib/formatting";
import { PageContext } from "../page";
import { Button } from "pkg/web/lib/button";
import { shallow_copy } from "pkg/web/lib/utils";
import { EditInput } from "pkg/web/lib/input";
import { PropertiesTable } from "../properties_table";
import { run_machine_command } from "../rpc_utils";

export class SettingsComponent extends React.Component<{ machine: any, context: PageContext }> {

    _run_command = (command, done) => {
        run_machine_command(this.props.context, this.props.machine, command, done);
    }

    render() {
        let machine = this.props.machine;

        let properties = [
            {
                name: 'Name:',
                value: <EditInput value={machine.config.name || ''} onChange={(value, done) => {
                    this._run_command({
                        update_config: { name: value }
                    }, done);
                }} />
            }
        ];

        return (
            <div>
                <div className="card">
                    <div className="card-header">
                        General
                    </div>
                    <div className="card-body">
                        <PropertiesTable properties={properties} style={{ verticalAlign: 'baseline' }} />

                        <div>
                            {/* TODO: This needs confirmation followed by navigating away from the current page. */}
                            <Button onClick={(done) => { }} preset="danger">Delete Machine</Button>
                        </div>
                    </div>
                </div>

            </div>

        );

    }

}