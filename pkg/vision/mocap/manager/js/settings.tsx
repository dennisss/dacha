import React from "react";
import { PageContext } from "pkg/web/lib/page";
import { Title } from "pkg/web/lib/title";
import { Navbar, NAVBAR_HEIGHT } from "./navbar";
import { MocapWorldViewer } from "./world_viewer";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";
import { Button } from "pkg/web/lib/button";
import { center_points } from "./utils";
import { round_nested_digits } from "pkg/web/lib/formatting";
import { Setting } from "pkg/web/lib/settings";
import { PropertiesTable } from "pkg/cnc/monitor/js/properties_table";
import { DARK_MODE } from "pkg/web/lib/dark";

// TODO: Continously fetch the manager status across all pages.

export interface SettingsPageProps {
    context: PageContext,
}

interface SettingsPageState {
    status: any,
}

export class SettingsPage extends React.Component<SettingsPageProps, SettingsPageState> {

    state = {
        status: null,

    }

    constructor(props: SettingsPageProps) {
        super(props);
    }

    componentDidMount(): void {
        this._get_status();
    }

    _get_status = async () => {
        if (this.props.context.channel.aborted()) {
            return;
        }

        try {
            await this._get_status_once();
        } catch (e) {
            console.error(e);
        }

        setTimeout(this._get_status, 2000);
    }

    async _get_status_once() {
        let res = await this.props.context.channel.call('mocap.Manager', 'Status', {});
        if (!res.status.ok()) {
            throw res.status.toString();
        }

        this.setState({ status: res.responses[0] });
    }

    _execute = async (req, done) => {

        try {
            let res = await this.props.context.channel.call('mocap.Manager', 'Execute', req);
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            await this._get_status_once();

        } finally {
            done()
        }
    }


    /*
    UI Settings
        - Dark Mode

    Network Interface

    RPC Server
        - TCP Port
        - Autostart
        - Status:
        - Start|Stop

    Verbose Settings
    */

    _render_rpc_card() {
        // TODO: Must also show the config.

        let make_button = (name, onClick, disabled) => {
            return <Button preset="outline-primary" onClick={onClick} style={{ marginRight: 10 }} disabled={disabled}>{name}</Button>
        };

        let status = this.state.status.aux_rpc_server;

        return (
            <Card header="RPC Server" style={{ marginBottom: 10 }}>
                <CardBody>
                    {make_button('Start', (done) => this._execute({ start_aux_rpc_server: true }, done), status.running)}
                    {make_button('Stop', (done) => this._execute({ stop_aux_rpc_server: true }, done), !status.running)}
                </CardBody>
                <div>
                    {this._render_object_table(this.state.status.aux_rpc_server)}
                </div>



            </Card>

        );
    }

    // TODO: Dedup me.
    _render_object_table(obj) {
        return (
            <div style={{ padding: '0 8px' }}>
                <table className="table" style={{ margin: 0 }}>
                    <tbody>
                        {Object.keys(obj).map((key) => {
                            return (
                                <tr key={key}>
                                    <td style={{ width: 1, whiteSpace: 'nowrap' }}>{key}</td>
                                    <td>{JSON.stringify(round_nested_digits(obj[key], 2))}</td>
                                </tr>
                            );
                        })}
                    </tbody>
                </table>
            </div>
        );
    }

    render() {

        if (!this.state.status) {
            return <div></div>;
        }

        // TODO: Make the body 'position: fixed' (probably just have this always happen).

        return (
            <div>
                <Title value="Mocap | Settings" />
                <Navbar />

                <div className="container" style={{ marginTop: 20 }}>
                    {this._render_rpc_card()}
                </div>

            </div>
        );
    }
};
