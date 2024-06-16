import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { ModalStore } from "pkg/web/lib/modal";

export async function pick_file(channel: Channel): Promise<string> {

    let files_res = await channel.call('cnc.Monitor', 'QueryEntities', { entity_type: 'FILE' });
    if (!files_res.status.ok()) {
        throw files_res.status.toString();
    }

    let files = files_res.responses[0].files || [];

    // TODO: Need to display a custom message if no files are available

    return await new Promise((res, rej) => {
        ModalStore.global().open({
            title: 'Pick a file:',
            content_style: { overflow: 'hidden' },
            body: (
                <div>
                    <ul className="list-group list-group-flush">
                        {files.map((file) => {
                            return <FileListItem key={file.id} file={file} onClick={() => {
                                ModalStore.global().close();
                                res(file.id);
                            }} />
                        })}
                    </ul>
                </div>
            )
        });
    });
}

class FileListItem extends React.Component<{ file: any, onClick: any }> {
    render() {
        let data = this.props.file;

        return (
            <li className="list-group-item list-group-item-action" style={{ cursor: 'pointer' }} onClick={this.props.onClick}>
                <div>{data.name}</div>
            </li>
        );
    }
}