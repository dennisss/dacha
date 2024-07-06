import React from "react";
import { PageContext } from "../page";
import { watch_entities } from "../rpc_utils";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "../navbar";
import { timestamp_proto_to_millis } from "pkg/web/lib/formatting";
import { ProgramRunFileBox } from "./file";
import { ProgramRunStatsBox } from "./stats";
import { MetricsBox } from "../machine/metrics";
import { ProgramRunVideoBox } from "./video";
import { ProgramRunTimelineBox } from "./timeline";
import { Card } from "../card";
import { Router } from "pkg/web/lib/router";


interface ProgramRunPageProps {
    machine_id: string
    run_id: string
    context: PageContext
}

interface ProgramRunPageState {
    _machine: any
    _run: any
}

export class ProgramRunPage extends React.Component<ProgramRunPageProps, ProgramRunPageState> {

    state = {
        _machine: null,
        _run: null
    }

    constructor(props: any) {
        super(props);

        watch_entities(props.context, { entity_type: 'MACHINE', entity_id: props.machine_id, verbose: false }, (msg) => {
            if (msg.machines.length != 1) {
                throw 'Unable to find the machine';
            }

            let m = msg.machines[0];
            this.setState({ _machine: m });
        })

        // TODO: Also watch the run?
        this._get_run();
    }

    async _get_run() {
        let res = await this.props.context.channel.call('cnc.Monitor', 'GetRunHistory', { machine_id: this.props.machine_id });

        if (!res.status.ok()) {
            throw res.status.toString();
        }

        let msg = res.responses[0];
        msg.runs.map((run) => {
            if (run.run_id == this.props.run_id) {
                this.setState({ _run: run });
            }
        });
    }

    render() {
        let machine = this.state._machine;
        let run = this.state._run;
        if (!machine || !run) {
            return <div></div>;
        }

        let context = this.props.context;

        let start_time = new Date(timestamp_proto_to_millis(run.start_time));
        let end_time = run.end_time ? new Date(timestamp_proto_to_millis(run.end_time)) : undefined;

        let machine_name = machine.config.name || 'Untitled Machine';

        // TODO: If today's date, show the time string.
        let title = machine_name + ' @ ' + start_time.toLocaleDateString();

        let machine_link = '/ui/machines/' + machine.id;

        return (
            <div>
                <Title value={title} />
                <Navbar extraLink={{
                    name: title,
                    to: '/ui/machines/' + machine.id + '/runs/' + run.run_id
                }} />

                <div className="container-fluid">
                    <div className="row" style={{ padding: '10px 0' }}>
                        <div className="col col-md-3">
                            <a className="nostyle" href={machine_link} onClick={(e) => {
                                e.preventDefault();
                                Router.global().goto(machine_link);
                            }}>
                                <div className="card card-link" style={{ cursor: 'pointer', marginBottom: 10 }}>
                                    <div className="card-header">
                                        Machine: {machine_name}
                                    </div>
                                </div>
                            </a>


                            <ProgramRunFileBox run={run} context={context} />
                            <ProgramRunStatsBox run={run} context={context} />
                        </div>
                        <div className="col col-md-9">
                            <ProgramRunTimelineBox run={run} context={context} />
                            <ProgramRunVideoBox context={context} machine={machine} run={run} />
                            <MetricsBox context={context} machine={machine} startTime={start_time} endTime={end_time} />
                        </div>
                    </div>
                </div>
            </div>

        );
    }
};






