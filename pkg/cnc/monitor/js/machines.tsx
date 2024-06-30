import React from "react";
import { Router } from "pkg/web/lib/router";
import { PageContext } from "./page";
import { watch_entities } from "./rpc_utils";
import { PropertiesTable } from "./properties_table";
import { get_player_properties } from "./machine/player";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "./navbar";


export interface MachinesPageProps {
    context: PageContext,
}

interface MachinesPageState {
    _machines: object[] | null
}

export class MachinesPage extends React.Component<MachinesPageProps, MachinesPageState> {

    state = {
        _machines: null
    }

    constructor(props: MachinesPageProps) {
        super(props);

        watch_entities(props.context, { entity_type: 'MACHINE' }, (msg) => {
            let machines = msg.machines || [];
            machines.sort((a, b) => {
                a['id'] < b['id']
            });

            this.setState({ _machines: machines });
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
                    <div style={{ fontWeight: 'bold', paddingBottom: 15 }}>
                        Available Machines:
                    </div>

                    {machines.map((machine) => {
                        return <MachineCard key={machine['id']} machine={machine} />;
                    })}

                    <hr />

                    <div style={{ fontWeight: 'bold', paddingBottom: 15 }}>
                        Disconnected Machines:
                    </div>
                </div>
            </div>
        );
    }

};

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
            <div className="card" style={{ marginBottom: 20, cursor: 'pointer' }} onClick={this._on_click}>
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
        );

    }

};
