import React from "react";
import { PropertiesTable, Property } from "../properties_table";
import { PageContext } from "pkg/web/lib/page";
import { run_machine_command } from "../rpc_utils";
import { TimeUnit, format_duration_proto, format_duration_secs, timestamp_proto_to_millis } from "pkg/web/lib/formatting";
import { Button } from "pkg/web/lib/button";
import { pick_file } from "../file_picker";
import { Card, CardBody } from "../card";
import { MachineUiState, ProgramPreviewContainer, ProgramPreviewData } from "./state";
import { parse_binary_images } from "./binary_image";

export interface PlayerBoxProps {
    context: PageContext;
    machine: any;
    ui_state: MachineUiState;
}

export class PlayerBox extends React.Component<PlayerBoxProps> {

    render() {
        let machine = this.props.machine;

        return (
            <Card id="program" header="Program Playback" style={{ marginBottom: 10 }}>
                <CardBody>
                    {machine.state.loaded_program ? (
                        <PlayerLoaded {...this.props} />
                    ) : (
                        <PlayerNotLoaded {...this.props} />
                    )}

                </CardBody>
            </Card>
        );
    }
};

class PlayerLoaded extends React.Component<PlayerBoxProps> {

    _click_macro_button = (command, done) => {
        run_machine_command(this.props.context, this.props.machine, command, done);
    }

    _open_file = (done) => {
        run_open_file(this.props.context, this.props.machine, () => { });
        done();
    }

    // NOTE: This assumes that the preview is already in a loaded state in the machine proto
    _load_preview_data(): ProgramPreviewContainer {
        let ui_state = this.props.ui_state;
        let preview = this.props.machine.state.loaded_program.preview;

        let existing_data = ui_state.program_preview();
        if (existing_data && existing_data.config_key == preview.config_hash && existing_data.file_id == preview.file_id && existing_data.revision == preview.revision) {
            return existing_data;
        }

        // TODO: Cancel/abort any existing data loading.

        let container: ProgramPreviewContainer = {
            config_key: preview.config_hash,
            file_id: preview.file_id,
            revision: preview.revision,
            loading: true
        };
        ui_state.set_program_preview(container);

        new Promise(async () => {
            try {
                let layer_images = [];

                if (preview.layer_images_url) {
                    let res = await fetch(preview.layer_images_url);
                    let buf = await res.arrayBuffer();

                    // TODO: Deduplicate this.
                    if (!res.ok) {
                        if (res.body !== null) {
                            res.body.cancel();
                        }

                        throw new Error('Error returned in GET request: ' + res.status + ': ' + res.statusText);
                    }

                    layer_images = parse_binary_images(buf);
                }

                ui_state.set_program_preview({
                    config_key: preview.config_hash,
                    file_id: preview.file_id,
                    revision: preview.revision,
                    data: {
                        layer_images
                    },
                    loading: false
                });
            } catch (e) {
                ui_state.set_program_preview({
                    config_key: preview.config_hash,
                    file_id: preview.file_id,
                    revision: preview.revision,
                    error: (e + '')
                });
            }
        });

        return container;
    }

    _render_preview_progress() {

        let preview = this.props.machine.state.loaded_program.preview;

        // TODO: Clear any preview cache.
        let regenerate_button = (
            <Button preset="light"
                style={{ float: 'right', marginLeft: '10px' }}
                onClick={(done) => this._click_macro_button({ regenerate_preview: true }, done)}
            >
                <span className="material-symbols-fill">refresh</span>
            </Button>

        )

        let error = preview.state.error;

        if (preview.state.ready) {
            let data = this._load_preview_data();

            if (data.error) {
                error = data.error;
            } else if (data.loading) {
                //
            } else {
                let n_layers = (preview.layers || []).length;
                return (
                    <div>
                        {regenerate_button}
                        Processed all {n_layers} layer{n_layers == 1 ? '' : 's'}
                    </div>
                );
            }
        }

        if (error) {
            return (
                <div>
                    {regenerate_button}
                    <span style={{ color: 'red', fontSize: '0.8em' }}>
                        Error: {error}
                    </span>
                </div>

            );
        }

        let p = Math.round((preview.state.progress || 0) * 100);
        return (
            <div>
                <div className="progress">
                    <div className={"progress-bar"} style={{ width: (p + '%'), minWidth: 10 }}></div>
                </div>
                <div style={{ fontSize: '0.8em' }}>{p}% done generating preview</div>
            </div>
        );
    }

    render() {
        let machine = this.props.machine;

        let state = 'STOPPED';
        if (machine.state.running_program) {
            state = machine.state.running_program.status;
        }

        let can_play = state == 'STOPPED' || state == 'DONE' || state == 'PAUSED' || state == 'ERROR';
        let is_playing = state == 'PLAYING' || state == 'STARTING';

        let can_pause = state == 'PLAYING' || state == 'STARTING';
        let is_paused = state == 'PAUSING' || state == 'PAUSED';

        let can_stop = state == 'PLAYING' || state == 'PAUSING' || state == 'PAUSED' || state == 'STARTING';
        let is_stopped = state == 'DONE' || state == 'STOPPED' || state == 'ERROR';

        let properties = get_player_properties(machine);

        properties.push({
            name: 'Preview:',
            value: this._render_preview_progress()
        });

        console.log(machine.state);

        return (
            <div>
                <PropertiesTable properties={properties} />

                <div style={{ width: '100%', display: 'flex' }}>
                    <div className="btn-group" style={{ flexGrow: 1 }}>
                        <PlayerButton active={is_playing} disabled={!can_play}
                            onClick={(done) => this._click_macro_button({ play_program: true }, done)}>
                            <span className="material-symbols-fill">play_arrow</span>
                        </PlayerButton>
                        <PlayerButton active={is_paused} disabled={!can_pause}
                            onClick={(done) => this._click_macro_button({ pause_program: true }, done)}>
                            <span className="material-symbols-fill">pause</span>
                        </PlayerButton>
                        <PlayerButton active={is_stopped} disabled={!can_stop}
                            onClick={(done) => this._click_macro_button({ stop_program: true }, done)}>
                            <span className="material-symbols-fill">stop</span>
                        </PlayerButton>
                    </div>
                    <div style={{ marginLeft: 5 }}>
                        <Button preset="outline-secondary" disabled={!is_stopped} onClick={this._open_file}>
                            <span className="material-symbols-outlined">file_open</span>
                            <span style={{ marginLeft: '1ex', verticalAlign: 'top' }}>Open File</span>
                        </Button>
                    </div>

                </div>

            </div>
        );

    }

}

export function get_player_properties(machine: any, thin: boolean = false): Property[] {
    return get_program_run_properties(machine.state.running_program, machine.state.loaded_program.file, thin);
}

export function get_program_run_properties(run: any | null, file: any | null, thin: boolean = false): Property[] {

    // TODO: Dedup this.
    let state = 'STOPPED';
    if (run) {
        state = run.status;
    }
    let is_stopped = state == 'DONE' || state == 'STOPPED' || state == 'ERROR';


    let properties = [
        {
            name: 'File:',
            value: file.name
        },
    ];

    let state_view = <span>{state}</span>;
    if (state == 'ERROR') {
        state_view = <span style={{ fontWeight: 'bold', color: 'red' }}>{state}</span>;
    }

    if (!thin) {
        properties.push({
            name: 'State:',
            value: state_view
        });
    }

    if (run) {
        // TODO: Switch to a CardError?
        if (run.status_message) {
            properties.push({
                name: 'Message:',
                value: run.status_message.text || ''
            });
        }

        let percentage = Math.round((run.progress || 0) * 100);

        let line_number = run.line_number || 0;
        let num_lines = file.program.num_lines;

        // TODO: bg-primary if DONE
        // TODO: bg-info is PAUSED
        // TODO: bg-dark is STOPPED
        properties.push({
            name: 'Progress:',
            value: (
                <div>
                    <div className="progress">
                        <div className={"progress-bar" + (state == 'ERROR' ? ' bg-danger' : '')} style={{ width: (percentage + '%'), minWidth: 10 }}></div>
                    </div>
                    <div style={{ fontSize: '0.8em' }}>{thin ? <span>{state_view}&nbsp;|&nbsp;</span> : null}{percentage}% of time | {line_number} / {num_lines} lines</div>
                </div>


            )
        });

        // TODO: Start showing the wall time in addition to the playing time (by checking the segments.)
        {
            properties.push({
                name: 'Elapsed:',
                value: run_elapsed_time(run)
            });
        }

        if (!is_stopped && run.estimated_remaining_time) {
            let v = format_duration_proto(run.estimated_remaining_time, TimeUnit.Minute);

            if (thin) {
                properties[properties.length - 1].value += ' (' + v + ' remaining)';
            } else {
                properties.push({
                    name: 'ETA:',
                    value: v
                });
            }
        }
    }

    return properties;
}

export function run_elapsed_time(run: any): string {
    let value = '<unknown>';
    let start_time = timestamp_proto_to_millis(run.start_time);

    let end_time = timestamp_proto_to_millis(run.end_time || run.last_updated);
    value = format_duration_secs((end_time - start_time) / 1000, TimeUnit.Minute);

    return value;
}

class PlayerButton extends React.Component<{ active: boolean, disabled: boolean, onClick: any }> {
    render() {
        const disabled = this.props.disabled;
        const active = this.props.active;

        return (
            <Button style={{ opacity: (disabled && !active ? 0.2 : (active ? 1 : null)) }} preset={active ? "dark" : "outline-dark"} active={active} disabled={this.props.disabled} onClick={this.props.onClick}>
                {this.props.children}
            </Button>
        );
    }
}

async function run_open_file(context: PageContext, machine: any, done: any) {
    try {
        let file_id = await pick_file(context.channel);

        await run_machine_command(context, machine, {
            load_program: {
                file_id: file_id
            }
        }, done);
        return;

    } catch (e) {
        // TODO: notificaiton
        console.error(e);
    }

    done()
}

class PlayerNotLoaded extends React.Component<PlayerBoxProps> {
    _open_file = (done) => {
        run_open_file(this.props.context, this.props.machine, () => { });
        done();
    }

    render() {
        return (
            <div>
                <div style={{ margin: '10px 10px 20px 10px', color: '#888', textAlign: 'center' }}>
                    No program loaded.
                </div>

                <Button preset="outline-primary" style={{ width: '100%' }} onClick={this._open_file} >Open File</Button>
            </div>
        );
    }

}
