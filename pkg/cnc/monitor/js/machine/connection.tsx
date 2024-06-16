import React from "react";
import { PropertiesTable } from "../properties_table";
import { PageContext } from "../page";
import { run_machine_command } from "../rpc_utils";
import { Button } from "pkg/web/lib/button";


export interface ConnectionBoxProps {
    context: PageContext,
    machine: any
}

export class ConnectionBox extends React.Component<ConnectionBoxProps> {

    _run_command = (command, done) => {
        run_machine_command(this.props.context, this.props.machine, command, done);
    }

    render() {
        let machine = this.props.machine;
        let connection_state = machine.state.connection_state;

        let properties = [
            {
                name: 'State:',
                value: connection_state
            }
        ];

        if (connection_state == 'ERROR' && machine.state.last_connection_error) {
            properties.push({
                name: 'Error:',
                value: machine.state.last_connection_error
            })
        }

        let auto_connect = machine.config.auto_connect || false;

        properties.push(
            {
                name: 'Auto Connect:',
                value: (
                    <div>
                        <div className="form-check form-switch">
                            {/* TODO: Name this a stateful switch with a spinner */}
                            <input className="form-check-input" type="checkbox" checked={auto_connect} onChange={(e) => {
                                let new_value = e.target.checked;
                                this._run_command({ update_config: { auto_connect: new_value } }, () => { });
                            }} />
                        </div>

                        <div style={{ fontSize: '0.8em' }}>
                            Selector: {JSON.stringify(machine.config.device)}
                        </div>
                    </div>
                )
            }
        );

        // If autoconnecting, show the spec. 
        // (auto-connect only supported if bound to the machine).
        //  <input class="form-check-input" type="checkbox" value="" id="flexCheckDisabled" disabled>

        let can_disconnect = connection_state == 'CONNECTED' || connection_state == 'CONNECTING';
        let can_connect = !can_disconnect && (machine.state.connection_device ? true : false);

        // TODO: Show the connector selector proto.

        // TODO: May need a dropdown of available devices if it is not clear.

        // TODO: Need disconnect confirmation if we are playing.

        /*
        State
        Path to the serial port.
        USB info
        Serial Number
        Baud Rate
        Connect / Disconnect button depending on the state.
        Auto-Connect Toggle.
        */

        return (
            <div className="card" style={{ marginBottom: 10 }}>
                <div className="card-header">
                    Connection
                </div>

                <div className="card-body">
                    <PropertiesTable properties={properties} />

                    <div>
                        {can_disconnect ? (
                            <Button preset="outline-dark" style={{ width: '100%' }}
                                onClick={(done) => this._run_command({ disconnect: true }, done)}>Disconnect</Button>
                        ) : null}
                        {can_connect ? (
                            <Button preset="outline-primary" style={{ width: '100%' }}
                                onClick={(done) => this._run_command({ connect: true }, done)}>Connect</Button>
                        ) : null}
                    </div>

                </div>


            </div>
        );
    }
};
