import React from "react";
import { PropertiesTable } from "../properties_table";
import { PageContext } from "../page";
import { run_machine_command } from "../rpc_utils";
import { Button } from "pkg/web/lib/button";
import { CardError } from "../card_error";
import { DeviceSelectorInput } from "../device_selector";
import { shallow_copy } from "pkg/web/lib/utils";
import { Card, CardBody } from "../card";


export interface ConnectionBoxProps {
    context: PageContext,
    machine: any
}

interface ConnectionBoxState {
    // Contains a sparse edit to the MachineConfig.
    _config: any,

    _selected_device: any
}

export class ConnectionBox extends React.Component<ConnectionBoxProps, ConnectionBoxState> {

    state = {
        _config: null,
        _selected_device: null
    };

    _run_command = (command, done) => {
        run_machine_command(this.props.context, this.props.machine, command, done);
    }

    _click_save = (done) => {
        this._run_command({ update_config: this.state._config }, () => {
            this._click_revert(done);
        });
    }

    _click_revert = (done) => {
        this.setState({
            _config: null,
            _selected_device: null
        }, () => {
            done()
        });
    }

    render() {
        let machine = this.props.machine;

        let machine_config = { ...machine.config, ...(this.state._config || {}) };

        let connection_state = machine.state.connection_state;

        let properties = [
            {
                name: 'State:',
                value: connection_state
            }
        ];

        let auto_connect = machine_config.auto_connect || false;

        properties.push(
            {
                name: 'Auto Connect:',
                value: (
                    <div>
                        <div className="form-check form-switch">
                            <input className="form-check-input" type="checkbox" checked={auto_connect} onChange={(e) => {
                                let new_value = e.target.checked;

                                let config = shallow_copy(this.state._config || {});
                                config.auto_connect = new_value;

                                this.setState({
                                    _config: config
                                });
                            }} />
                        </div>
                    </div>
                )
            }
        );

        let can_disconnect = connection_state == 'CONNECTED' || connection_state == 'CONNECTING';
        let can_connect = !can_disconnect && (machine.state.connection_device ? true : false);

        // TODO: Need disconnect confirmation if we are playing.

        /*
        Dealing with the transition state:
        - Show the selector with both the wanted and actual values. 
        */

        return (
            <Card
                id="connection"
                style={{ marginBottom: 10 }}
                header="Connection"
                error={machine.state.last_connection_error}
            >
                <CardBody>
                    <div>
                        <div style={{ fontWeight: 'bold', paddingBottom: 5 }}>
                            General
                        </div>
                        <PropertiesTable properties={properties} />
                    </div>

                    <div>
                        <div style={{ fontWeight: 'bold', paddingBottom: 5 }}>
                            Device Selector
                        </div>

                        {/* TODO: state.connection_device won't match the selector if we are transitioning between devices */}
                        <DeviceSelectorInput
                            context={this.props.context}
                            device={this.state._selected_device || machine.state.connection_device}
                            selector={machine_config.device}
                            connected_device={machine.state.connection_device}
                            filter={(device) => device.serial_path ? true : false}
                            onChange={(selector, device) => {
                                let config = shallow_copy(this.state._config || {});
                                config.device = selector;
                                config.clear_fields = [{ key: [{ field_name: 'device' }] }];
                                this.setState({ _config: config, _selected_device: device });
                            }}
                        />
                    </div>

                    <div>
                        {this.state._config != null ? (
                            <div style={{ display: 'flex' }}>
                                <Button style={{ flexGrow: 1, marginRight: 5 }} preset="primary" onClick={this._click_save}>
                                    Save
                                </Button>
                                <Button style={{ flexGrow: 1, marginLeft: 5 }} preset="dark" onClick={this._click_revert}>
                                    Revert
                                </Button>
                            </div>
                        ) : null}

                        {can_disconnect && this.state._config == null ? (
                            <Button preset="outline-dark" style={{ width: '100%' }}
                                onClick={(done) => this._run_command({ disconnect: true }, done)}>Disconnect</Button>
                        ) : null}
                        {can_connect && this.state._config == null ? (
                            <Button preset="outline-primary" style={{ width: '100%' }}
                                onClick={(done) => this._run_command({ connect: true }, done)}>Connect</Button>
                        ) : null}
                    </div>
                </CardBody>
            </Card>
        );
    }
};
