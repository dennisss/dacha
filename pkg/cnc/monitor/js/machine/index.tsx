import React from "react";
import { PageContext } from "../page";
import { watch_entities } from "../rpc_utils";
import { ControlsComponent } from "./controls";
import { TerminalComponent } from "./terminal";
import { PositionBox } from "./position";
import { ConnectionBox } from "./connection";
import { PlayerBox } from "./player";
import { SettingsComponent } from "./settings";
import { CameraBox } from "./camera";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "../navbar";

interface MachinePageProps {
    id: string
    context: PageContext
}

interface MachinePageState {
    _machine: any
}

export class MachinePage extends React.Component<MachinePageProps, MachinePageState> {

    state = {
        _machine: null,
        _right_tab: 0
    }

    constructor(props: any) {
        super(props);

        watch_entities(props.context, { entity_type: 'MACHINE', entity_id: props.id, verbose: true }, (msg) => {
            if (msg.machines.length != 1) {
                throw 'Unable to find the machine';
            }

            let m = msg.machines[0];
            console.log(m);

            this.setState({ _machine: m });
        })
    }

    render() {
        let machine = this.state._machine;
        if (!machine) {
            return <div></div>;
        }


        let tabs = [
            {
                name: 'Controls',
                view: <ControlsComponent machine={machine} context={this.props.context} />
            },
            {
                name: 'Terminal',
                view: <TerminalComponent machine={machine} context={this.props.context} />
            },
            {
                name: 'History',
                view: <div></div>
            },
            {
                name: 'Settings',
                view: <SettingsComponent machine={machine} context={this.props.context} />
            },
        ];

        let active_tab = tabs[this.state._right_tab];

        let machine_name = machine.config.name || 'Untitled Machine';

        return (
            <div>
                <Title value={machine_name} />
                <Navbar extraLink={{
                    name: machine_name,
                    to: '/ui/machines/' + machine.id
                }} />

                <div className="container-fluid">
                    <div className="row" style={{ padding: '10px 0' }}>
                        <div className="col col-md-3">
                            <CameraBox machine={machine} context={this.props.context} />
                            <ConnectionBox machine={machine} context={this.props.context} />
                            <PlayerBox machine={machine} context={this.props.context} />
                        </div>
                        <div className="col col-md-6">
                            <PositionBox machine={machine} context={this.props.context} />
                        </div>
                        <div className="col col-md-3">
                            <div style={{ marginBottom: 15 }}>
                                <ul className="nav nav-tabs">
                                    {tabs.map((tab, i) => {
                                        return (
                                            <li className="nav-item" key={i}>
                                                <a className={"nav-link" + (active_tab == tab ? " active" : "")}
                                                    href="#"
                                                    onClick={(e) => {
                                                        e.preventDefault();
                                                        this.setState({ _right_tab: i });
                                                    }}
                                                >
                                                    {tab.name}
                                                </a>
                                            </li>
                                        );
                                    })}
                                </ul>
                            </div>

                            {active_tab.view}
                        </div>
                    </div>
                </div>
            </div>

        );
    }
};






