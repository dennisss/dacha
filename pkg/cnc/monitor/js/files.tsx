import React from "react";
import { PropertiesTable } from "./properties_table";
import { Button } from "pkg/web/lib/button";
import { format_bytes_size, format_duration_proto, timestamp_proto_to_millis } from "pkg/web/lib/formatting";
import { pick_machine } from "./machine_picker";
import { watch_entities } from "./rpc_utils";
import { PageContext } from "./page";
import { Router } from "pkg/web/lib/router";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "./navbar";

interface FilesPageState {
    _files: object[] | null
    _pending_uploads: any[]
}

export interface FilesPageProps {
    context: PageContext
}

export function sort_files_list(files: any[]) {
    let now = (new Date()).getTime();
    files.sort((a, b) => {
        let a_time = now;
        if (a.upload_time) {
            a_time = timestamp_proto_to_millis(a.upload_time);
        }

        let b_time = now;
        if (b.upload_time) {
            b_time = timestamp_proto_to_millis(b.upload_time);
        }

        return b_time - a_time;
    })
}

export class FilesPage extends React.Component<FilesPageProps, FilesPageState> {
    state = {
        _files: null,
        _pending_uploads: []
    }

    constructor(props: FilesPageProps) {
        super(props);

        watch_entities(this.props.context, { entity_type: 'FILE' }, (msg) => {
            let files = msg.files || [];
            sort_files_list(files);
            this.setState({ _files: files });
        });
    }

    _on_pick_file = (file: File) => {
        let notif = this.props.context.notifications.add({
            preset: 'primary',
            cancellable: false,
            text: `Starting upload for ${file.name}`
        })

        this._do_upload(notif, file).catch((e) => {
            notif.update({
                text: e + '',
                cancellable: true,
                preset: 'danger'
            });
        })
    }

    // TODO: Need to implement cancellation of file uploads:
    // - Probably pass in a AbortSignal which we can 
    async _do_upload(notif: Notification, file: File) {
        let start_res = await this.props.context.channel.call('cnc.Monitor', 'StartFileUpload', { name: file.name, size: file.size });

        if (!start_res.status.ok()) {
            throw `Failed to start upload for ${file.name}`;
        }

        notif.update({
            text: `Uploading data for ${file.name}`
        })

        let upload_res = await fetch('/api/files/upload?id=' + start_res.responses[0].file.id, {
            method: 'POST',
            body: file,
        });

        if (!upload_res.ok) {
            if (upload_res.body) {
                upload_res.body.cancel();
            }

            throw `File data upload failed`;
        }

        notif.remove();
    }

    render() {
        let files = this.state._files || [];

        let ctx = this.props.context;

        // TODO: Need some sorting capabilities and a way to deal with very long lists.

        // (by default sort by recently added ones).

        return (
            <div>
                <Title value="Files" />
                <Navbar />

                <div className="container" style={{ paddingTop: 20, paddingBottom: 20, position: 'relative' }}>
                    {/* <div style={{ textAlign: 'right', marginBottom: 16 }}>
                        <div className="row g-3 align-items-center">
                            <div className="col-auto">
                                <label className="col-form-label">Search</label>
                            </div>
                            <div className="col-auto">
                                <input type="text" className="form-control" />
                            </div>
                        </div>
                    </div> */}

                    {/* TODO: Hide while searching */}
                    <div>
                        <UploadBox onPickFile={this._on_pick_file} />
                    </div>

                    {files.map((file) => {
                        return <FileBox key={file['id']} file={file} context={ctx} />;
                    })}
                </div>
            </div>

        );

    }
}

interface UploadBoxProps {
    onPickFile: any
}

class UploadBox extends React.Component<UploadBoxProps> {

    _input_el: React.RefObject<HTMLInputElement>

    constructor(props: any) {
        super(props);
        this._input_el = React.createRef();
    }

    _on_click_outer = (e: any) => {
        this._input_el.current?.click();
    }


    _on_input_change = (e: any) => {
        let files: File[] = e.target.files;
        if (files.length == 0) {
            return;
        }

        let file = files[0];
        this.props.onPickFile(file);
    }

    _on_drop = (e: Event) => {
        e.preventDefault();
        e.stopPropagation();

        if (e.dataTransfer.items) {
            [...e.dataTransfer.items].forEach((item, i) => {
                // If dropped items aren't files, reject them
                if (item.kind === "file") {
                    const file = item.getAsFile();
                    this.props.onPickFile(file);
                }
            });
        } else {
            [...e.dataTransfer.files].forEach((file, i) => {
                this.props.onPickFile(file);
            });
        }
    }

    _on_drag_event = (e: Event) => {
        e.preventDefault();
        e.stopPropagation();
    }

    render() {

        return (
            <div style={{ padding: "20px 10px", border: '1px dashed #ccc', cursor: 'pointer', borderRadius: 5, marginBottom: 10 }} onClick={this._on_click_outer} onDrop={this._on_drop} onDragOver={this._on_drag_event} onDragEnter={this._on_drag_event} onDragLeave={this._on_drag_event}>
                <div style={{ textAlign: 'center', color: '#aaa' }}>
                    Click/drop to upload a file
                </div>

                <input value="" type="file" multiple style={{ display: 'none' }} ref={this._input_el} onChange={this._on_input_change} />
            </div>
        );

    }
};


interface FileBoxProps {
    file: any
    context: PageContext
}

class FileBox extends React.Component<FileBoxProps> {

    _on_click_load = async (done: any) => {
        done();

        let ctx = this.props.context;

        try {
            let machine_id = await pick_machine(ctx.channel);

            let res = await ctx.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: machine_id,
                load_program: {
                    file_id: this.props.file.id
                }
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }

            Router.global().goto('/ui/machines/' + machine_id);

        } catch (e) {
            console.error(e);
            // TODO: Make notification
        }
    }

    _on_click_delete = async (done: any) => {
        let ctx = this.props.context;
        try {
            let res = await ctx.channel.call('cnc.Monitor', 'DeleteFile', { file_id: this.props.file.id });
            if (!res.status.ok()) {
                throw res.status.toString();
            }
        } catch (e) {
            ctx.notifications.add({
                text: 'Failed to delete file: ' + e,
                preset: 'danger',
                cancellable: true
            });
        }

        done();
    }

    _on_click_download = (done) => {
        done();

        let file = this.props.file;
        var link = document.createElement('a');
        link.href = file.urls.raw_url;
        link.download = file.name;
        link.click();
    }

    _on_click_reprocess = async (done: any) => {
        let ctx = this.props.context;
        try {
            let res = await ctx.channel.call('cnc.Monitor', 'ReprocessFile', { file_id: this.props.file.id });
            if (!res.status.ok()) {
                throw res.status.toString();
            }
        } catch (e) {
            ctx.notifications.add({
                text: 'Failed to start reprocessing of file: ' + e,
                preset: 'danger',
                cancellable: true
            });
        }

        done();
    }

    render() {
        let file = this.props.file;

        let state_view = file['state'];
        if (state_view == 'READY') {

        }

        let type = 'Unknown';
        if (file['program']) {
            type = 'Program';
        }

        let percentage = ((file.state_progress || 0) * 100);

        let properties = [
            {
                name: 'Name:',
                value: file['name'] + ' (Id: ' + file['id'] + ')'
            },
            // TODO: Only display if not READY
            {
                name: 'State:',
                /* Possibly swap with a progress bar if loading */
                value: (file.state == 'READY' ? file.state : (
                    <div>
                        <div className="progress">
                            <div className="progress-bar" style={{ width: percentage + '%', minWidth: 10 }}></div>
                        </div>
                        <div style={{ fontSize: '0.8em' }}>{file.state}:&nbsp;{Math.floor(percentage)}%</div>
                    </div>
                )),
            },
            {
                name: 'Size:',
                value: format_bytes_size(file['size'])
            },
        ];

        if (file['program']) {
            let dur = format_duration_proto(file['program']['normal_duration']);
            if (file['program']['silent_duration']) {
                dur += ` (Silent: ${format_duration_proto(file['program']['silent_duration'])})`;
            }

            properties.push({
                name: 'Duration:',
                value: dur
            });
        }

        let button_style = { display: "block", marginBottom: 10, width: '100%' };

        let ready = file.state == 'READY';

        let errors = get_file_errors(file);

        // TODO: Show an error if there are invalid lines in the file.

        // TODO: Must  only allow loading if the file is a program.

        // TODO: Add a cancel button if there is an active upload from our machine.

        return (
            <div style={{ padding: 10, border: '1px solid #888', marginBottom: 10 }}>
                <div style={{ display: "flex" }}>
                    <div style={{ flexShrink: 1 }}>
                        {file.has_thumbnail ? (
                            <img src={file.urls.thumbnail_url} style={{ width: 200 }} />
                        ) : (
                            <div style={{ width: 200, height: 200 * (9 / 16), backgroundColor: "#ccc" }}></div>
                        )}
                    </div>

                    <div style={{ flexGrow: 1, padding: '0 10px' }}>
                        <PropertiesTable properties={properties} />
                        {errors.length > 0 && ready ? (

                            <div className="alert alert-danger" style={{ fontSize: '0.8em' }}>
                                File can't be loaded since it has processing errors:
                                <ul style={{ margin: 0 }}>
                                    {errors.map((e, i) => {
                                        return <li key={i}>{e}</li>
                                    })}
                                </ul>
                            </div>
                        ) : null}
                    </div>

                    <div style={{ flexShrink: 1 }}>
                        <Button preset="primary" onClick={this._on_click_load} style={button_style} disabled={!ready || errors.length != 0}>Load</Button>
                        <Button preset="light" onClick={this._on_click_download} style={button_style} disabled={!ready}>Download</Button>
                        <Button preset="light" onClick={this._on_click_delete} style={button_style}>Delete</Button>
                        <Button preset="light" onClick={this._on_click_reprocess} disabled={!ready} style={button_style}>Re-process</Button>
                    </div>
                </div>
            </div>
        );
    }
}

// TODO: Move this server side once we have a better file compatibility checking system.
export function get_file_errors(file: any) {
    let errors: string[] = [];
    if (file.state != 'READY') {
        errors.push('File is not still processing.');
        return errors;
    }

    if (file.processing_error) {
        errors.push('Failure while procesing the file: ' + file.processing_error);
        return errors;
    }

    if (!file.program) {
        errors.push('File is not a GCode program.');
        return errors;
    }

    let program = file.program;

    if ((program.num_invalid_lines || 0) > 0) {
        let s = program.num_invalid_lines > 1 ? 's' : '';
        errors.push('Failed to interpret ' + program.num_invalid_lines + ` line${s} in the file.`);
    }

    errors = errors.concat(program.first_failures || []);

    return errors;
}