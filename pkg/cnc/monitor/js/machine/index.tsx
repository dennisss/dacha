import React from "react";
import { PageContext } from "pkg/web/lib/page";
import { watch_entities } from "../rpc_utils";
import { ControlsComponent } from "./controls";
import { TerminalComponent } from "./terminal";
import { PositionBox } from "./position";
import { ConnectionBox } from "./connection";
import { PlayerBox } from "./player";
import { SettingsComponent } from "./settings";
import { CamerasBox } from "./camera";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "../navbar";
import { HistoryComponent } from "./history";
import { MetricsBox } from "./metrics";
import { CarveraBox } from "./carvera";
import { MachineUiState } from "./state";
import { ObjectsBox } from "./objects";
import { CameraLiveBox } from "./live";

interface MachinePageProps {
    id: string
    context: PageContext
}

export class MachinePage extends React.Component<MachinePageProps> {

    state = {
        _machine: null,
        _right_tab: 0
    }

    _ui_state: MachineUiState = new MachineUiState();

    constructor(props: any) {
        super(props);

        watch_entities(props.context, { entity_type: 'MACHINE', entity_id: props.id, verbose: true }, (msg) => {
            if (msg.machines.length != 1) {
                throw 'Unable to find the machine';
            }

            let m = msg.machines[0];
            this.setState({ _machine: m });
        })

        this._ui_state.add_listener(() => setTimeout(() => this.forceUpdate()));
    }

    render() {
        let machine = this.state._machine;
        if (!machine) {
            return <div></div>;
        }

        // TODO: Have a better place for this.
        document.body.parentNode.className = "noscrollbar";

        let ui_state = this._ui_state;

        let left_panel = (
            <>
                <CamerasBox machine={machine} context={this.props.context} ui_state={ui_state} />
                <ConnectionBox machine={machine} context={this.props.context} />
                <PlayerBox machine={machine} context={this.props.context} ui_state={ui_state} />
                <ObjectsBox machine={machine} context={this.props.context} ui_state={ui_state} />
                <CarveraBox machine={machine} context={this.props.context} ui_state={ui_state} />
            </>
        )

        let tabs = [
            {
                name: 'Controls',
                view: <ControlsComponent machine={machine} context={this.props.context} ui_state={ui_state} />
            },
            {
                name: 'Terminal',
                view: <TerminalComponent machine={machine} context={this.props.context} />
            },
            {
                name: 'History',
                view: <HistoryComponent machine={machine} context={this.props.context} />
            },
            {
                name: 'Settings',
                view: <SettingsComponent machine={machine} context={this.props.context} ui_state={ui_state} />
            },
        ];

        if (ui_state.left_collapsed) {
            tabs.push({
                name: "State",
                view: left_panel,
            })
        }

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
                        {ui_state.left_collapsed ? null : (
                            <div className="col col-md-3">
                                {left_panel}
                            </div>
                        )}
                        <div className={"col col-md-" + (ui_state.left_collapsed ? '9' : '6')}>
                            <CameraLiveBox machine={machine} context={this.props.context} ui_state={ui_state} />
                            <PositionBox machine={machine} context={this.props.context} ui_state={ui_state} />
                            <MetricsBox machine={machine} context={this.props.context} />
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






