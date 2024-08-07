import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { ModalStore } from "pkg/web/lib/modal";
import { get_file_errors, sort_files_list } from "./files";

export async function pick_file(channel: Channel): Promise<string> {

    let files_res = await channel.call('cnc.Monitor', 'QueryEntities', { entity_type: 'FILE' });
    if (!files_res.status.ok()) {
        throw files_res.status.toString();
    }

    let files = files_res.responses[0].files || [];
    sort_files_list(files);

    files = files.filter((f) => {
        return get_file_errors(f).length == 0;
    });

    // TODO: Need to display a custom message if no files are available

    return await new Promise((res, rej) => {
        ModalStore.global().open({
            title: 'Pick a file:',
            content_style: { overflow: 'hidden' },
            body: (
                <div>
                    <ul className="list-group list-group-flush">
                        {files.length == 0 ? (
                            <div style={{ color: '#ccc', padding: 15, textAlign: 'center' }}>
                                No files uploaded
                            </div>
                        ) : null}

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