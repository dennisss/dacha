import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { ModalStore } from "pkg/web/lib/modal";

export async function pick_machine(channel: Channel): Promise<string> {

    let machines_res = await channel.call('cnc.Monitor', 'QueryEntities', { entity_type: 'MACHINE' });
    if (!machines_res.status.ok()) {
        throw machines_res.status.toString();
    }

    let machines = machines_res.responses[0].machines || [];

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
                            return <MachineListItem key={machine.id} machine={machine} onClick={() => {
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
    machine: any
    onClick: any
}

class MachineListItem extends React.Component<MachineListItemProps> {
    render() {
        let data = this.props.machine;

        // TODO: Also need to render info on the right side like compatibility info and current state.
        return (
            <li className="list-group-item list-group-item-action" style={{ cursor: 'pointer' }} onClick={this.props.onClick}>
                <div>{data.config.name || 'Unnamed Machine'}</div>
                <div style={{ color: '#444', fontSize: '0.8em' }}>{data.config.model_name || '<unknown model>'}</div>
            </li>
        );
    }
}