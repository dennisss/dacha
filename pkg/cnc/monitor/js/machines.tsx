import React from "react";
import { Router } from "pkg/web/lib/router";
import { PageContext } from "pkg/web/lib/page";
import { watch_entities } from "./rpc_utils";
import { PropertiesTable } from "./properties_table";
import { get_player_properties } from "./machine/player";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "./navbar";
import { compare_values } from "pkg/web/lib/utils";
import { Button } from "pkg/web/lib/button";


export interface MachinesPageProps {
    context: PageContext,
}

interface MachinesPageState {
    _machines: object[] | null,
    _presets: object[] | null
}

export class MachinesPage extends React.Component<MachinesPageProps, MachinesPageState> {

    state = {
        _machines: null,
        _presets: null
    }

    constructor(props: MachinesPageProps) {
        super(props);

        watch_entities(props.context, { entity_type: 'MACHINE' }, (msg) => {
            let machines = msg.machines || [];
            machines.sort((a, b) => compare_values(a.id, b.id));
            this.setState({ _machines: machines });
        });

        watch_entities(props.context, { entity_type: 'PRESET' }, (msg) => {
            let presets = msg.presets || [];
            presets.sort((a, b) => compare_values(a.base_config, b.base_config));
            this.setState({ _presets: presets });
        });
    }

    render() {
        let machines = this.state._machines || [];

        /*
        TODO: Make the disconnected machines options work.

        TODO: Support creating a new machine from a preset and an available device descriptor.

        TODO: Support enabling/disabling auto-machine creation.

        */

        return (
            <div>
                <Title value="Machines" />
                <Navbar />

                <div className="container" style={{ paddingTop: 20, paddingBottom: 20 }}>
                    {this.state._presets ? (
                        <NewMachineForm presets={this.state._presets} context={this.props.context} />
                    ) : null}

                    <div style={{ fontWeight: 'bold', paddingBottom: 15 }}>
                        Available Machines:
                    </div>

                    {machines.map((machine) => {
                        if (machine.state.connection_state == 'MISSING') {
                            return null;
                        }

                        return <MachineCard key={machine['id']} machine={machine} />;
                    })}

                    <hr />

                    <div style={{ fontWeight: 'bold', paddingBottom: 15 }}>
                        Disconnected Machines:
                    </div>

                    {machines.map((machine) => {
                        if (machine.state.connection_state != 'MISSING') {
                            return null;
                        }

                        return <MachineCard key={machine['id']} machine={machine} />;
                    })}
                </div>
            </div>
        );
    }

};

interface NewMachineFormProps {
    presets: any[],
    context: PageContext,
}

class NewMachineForm extends React.Component<NewMachineFormProps> {

    state = {
        _name: '',
        _preset: ''
    }

    _click_create = async (done) => {
        try {
            // TODO: Need this to have a timeout.
            let res = await this.props.context.channel.call('cnc.Monitor', 'CreateMachine', {
                config: {
                    name: this.state._name,
                    base_config: this.state._preset
                }
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }

            Router.global().goto('/ui/machines/' + res.responses[0].machine_id);

        } catch (e) {
            this.props.context.notifications.add({
                text: 'Machine creation failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        done();
    }

    render() {
        let presets = this.props.presets;

        return (
            <div>
                <div style={{ fontWeight: 'bold', paddingBottom: 15 }}>
                    Create Machine:
                </div>

                <div>
                    <form className="row row-cols-lg-auto g-3 align-items-center">
                        <div className="col-12">
                            <div className="input-group">
                                <input type="text" className="form-control" placeholder="Machine Name"
                                    value={this.state._name}
                                    onChange={(e) => this.setState({ _name: e.target.value })} />
                            </div>
                        </div>

                        <div className="col-12">
                            <select value={this.state._preset} className="form-select"
                                onChange={(e) => this.setState({ _preset: e.target.value })}>
                                <option value="">Base Preset</option>
                                {presets.map((preset) => {
                                    return (
                                        <option key={preset.base_config} value={preset.base_config}>
                                            {preset.model_name} ({preset.base_config})
                                        </option>
                                    );
                                })}
                            </select>
                        </div>

                        <div className="col-12">
                            <Button preset="primary" onClick={this._click_create} disabled={!this.state._name || !this.state._preset}>
                                Create
                            </Button>
                        </div>
                    </form>

                </div>

                <hr />
            </div>
        );
    }

}


interface MachineCardProps {
    machine: any
}

class MachineCard extends React.Component<MachineCardProps> {

    _on_click = (e: any) => {
        e.preventDefault();
        Router.global().goto('/ui/machines/' + this.props.machine['id']);
    }

    render() {
        let m = this.props.machine;

        let state = m['state']['connection_state'];
        let state_color = '';
        if (state == 'PLAYING') {
            state_color = 'darkgreen';
        }
        if (state == 'ERROR') {
            state_color = 'RED';
        }


        let properties = [
            {
                name: 'State:',
                value: <span style={{ color: state_color }}>{state}</span>
            },
            {
                name: 'Model:',
                value: m.config.model_name
            }
        ];


        if (m.state.running_program) {
            properties = properties.concat(get_player_properties(m, true));
        }

        return (
            <a className="nostyle" href={'/ui/machines/' + this.props.machine['id']} onClick={this._on_click}>
                <div className="card card-link" style={{ marginBottom: 20, cursor: 'pointer' }}>
                    <div className="card-header">
                        {m.config.name || 'Unnamed Machine'}

                        <div style={{ float: 'right' }}>Id: {m['id']}</div>
                    </div>
                    <div className="card-body">
                        <PropertiesTable keyWidth={200} properties={properties} />

                        {/*
                        - Maybe an error message 'alert' if there is an issue.
                    
                        */}
                    </div>
                </div>
            </a>
        );

    }

};
