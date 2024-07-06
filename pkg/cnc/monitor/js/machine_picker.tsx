import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { ModalStore } from "pkg/web/lib/modal";
import { compare_values } from "pkg/web/lib/utils";

export async function pick_machine(channel: Channel, last_machine_id: any = null): Promise<string> {

    let machines_res = await channel.call('cnc.Monitor', 'QueryEntities', { entity_type: 'MACHINE' });
    if (!machines_res.status.ok()) {
        throw machines_res.status.toString();
    }

    let machines = machines_res.responses[0].machines || [];

    machines.sort((a, b) => {
        if (a.id == last_machine_id) {
            return -1;
        }

        return compare_values(a.id, b.id);
    })

    // TODO: Need to display a custom message if no machines are connected

    // TODO: Sort by compatible and available machines first.

    return await new Promise((res, rej) => {
        ModalStore.global().open({
            title: 'Pick a machine to load this file:',
            content_style: { overflow: 'hidden' },
            body: (
                <div>
                    <ul className="list-group list-group-flush">
                        {machines.map((machine) => {
                            return <MachineListItem key={machine.id} is_last_machine={machine.id == last_machine_id} machine={machine} onClick={() => {
                                ModalStore.global().close();
                                res(machine.id);
                            }} />
                        })}
                    </ul>
                </div>
            )
        });

    })


}

interface MachineListItemProps {
    machine: any;
    onClick: any;
    is_last_machine: boolean;
}

class MachineListItem extends React.Component<MachineListItemProps> {
    render() {
        let data = this.props.machine;

        // TODO: Also need to render info on the right side like compatibility info and current state.
        return (
            <li className="list-group-item list-group-item-action" style={{ cursor: 'pointer' }} onClick={this.props.onClick}>
                <div>
                    {data.config.name || 'Unnamed Machine'}

                    {this.props.is_last_machine ? (
                        <span className={"badge rounded-pill bg-secondary"} style={{ verticalAlign: 'text-bottom', marginLeft: 10 }}>
                            Used in this run
                        </span>
                    ) : null}

                </div>
                <div style={{ color: '#444', fontSize: '0.8em' }}>{data.config.model_name || '<unknown model>'}</div>
            </li>
        );
    }
}