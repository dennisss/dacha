import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { PageContext } from "../page";
import { Button } from "pkg/web/lib/button";
import { LabeledCheckbox } from "pkg/web/lib/checkbox";
import { Card, CardBody } from "../card";
import { timestamp_proto_to_millis } from "pkg/web/lib/formatting";
import { PropertiesTable } from "../properties_table";
import { run_elapsed_time } from "./player";
import { Router } from "pkg/web/lib/router";
import { SpinnerInline } from "pkg/web/lib/spinner";


export interface HistoryComponentProps {
    context: PageContext;
    machine: any
}

interface HistoryComponentState {
    _runs: any[]
    _loading: boolean;
}

export class HistoryComponent extends React.Component<HistoryComponentProps, HistoryComponentState> {

    state = {
        _loading: true,
        _runs: []
    }

    _abort_controller: AbortController = new AbortController();

    constructor(props: HistoryComponentProps) {
        super(props);

        this._read_history();
    }

    componentWillUnmount(): void {
        this._abort_controller.abort();
    }

    // TODO: Need infinite retrying.
    // TODO: Need cancellation on component unmount.
    async _read_history() {

        let res = await this.props.context.channel.call('cnc.Monitor', 'GetRunHistory', { machine_id: this.props.machine.id }, { abort_signal: this._abort_controller.signal });

        if (!res.status.ok()) {
            throw res.status.toString();
        }

        let msg = res.responses[0];
        this.setState({
            _runs: msg.runs || [],
            _loading: false
        });
    }

    render() {
        let machine = this.props.machine;
        let runs = this.state._runs;

        return (
            <div>
                {this.state._loading ? (
                    <div style={{ textAlign: 'center', padding: 10 }}>
                        <SpinnerInline />
                    </div>
                ) : (
                    runs.length == 0 ? (
                        <div style={{ textAlign: 'center', padding: 10, color: '#ccc' }}>
                            No runs recorded.
                        </div>
                    ) : null
                )}
                {runs.map((run) => {
                    let start_time = new Date(timestamp_proto_to_millis(run.start_time));

                    let properties = [
                        {
                            name: 'State:',
                            value: run.status
                        },
                        {
                            name: 'Elapsed:',
                            value: run_elapsed_time(run)
                        }
                    ];

                    if (run.file) {
                        properties.push({
                            name: 'File:',
                            value: run.file.name
                        });
                    }

                    let url = '/ui/machines/' + machine.id + '/runs/' + run.run_id;
                    let on_click = (e: any) => {
                        e.preventDefault();
                        Router.global().goto(url);
                    }

                    return (
                        <a href={url} className="nostyle" onClick={on_click}>
                            <Card key={run.run_id} header={start_time.toLocaleString()} style={{ overflow: 'hidden', cursor: 'pointer', marginBottom: 10 }} className="card-link">
                                <div style={{ padding: '0 8px', marginBottom: '-1px' }}>
                                    <PropertiesTable properties={properties} style={{ margin: 0, }} />
                                </div>
                            </Card>
                        </a>


                    );

                })}

            </div>
        )
    }
}

