import React from "react";
import { Channel } from "pkg/web/lib/rpc";
import { PageContext } from "../page";
import { Button } from "pkg/web/lib/button";
import { LabeledCheckbox } from "pkg/web/lib/checkbox";


export interface TerminalComponentProps {
    context: PageContext;
    machine: any
}

interface TerminalComponentState {
    _lines: string[]
    _command: string
    _sending_command: boolean
}

export class TerminalComponent extends React.Component<TerminalComponentProps, TerminalComponentState> {

    state = {
        _lines: [],
        _command: '',
        _sending_command: false,

        _show_ok_lines: false,
        _show_state_lines: false,
        _auto_scroll: true
    }

    _history_el: React.RefObject<HTMLDivElement>
    _abort_controller: AbortController = new AbortController();

    constructor(props: TerminalComponentProps) {
        super(props);

        this._history_el = React.createRef();
        this._read_log();
    }

    componentWillUnmount(): void {
        this._abort_controller.abort();
    }

    // TODO: Need infinite retrying.
    // TODO: Need cancellation on component unmount.
    async _read_log() {
        let res = this.props.context.channel.call_streaming('cnc.Monitor', 'ReadSerialLog', { machine_id: this.props.machine.id }, { abort_signal: this._abort_controller.signal });

        let first = true;
        while (true) {
            let msg = await res.recv();
            if (!msg) {
                break;
            }

            let previous_lines = this.state._lines;
            if (first) {
                first = false;
                previous_lines = [];
            }

            let lines = previous_lines.concat(msg.lines || []);
            if (lines.length > 500) {
                lines = lines.slice(lines.length - 500);
            }

            this.setState({ _lines: lines }, () => {
                // Scroll to the bottom.

                if (!this.state._auto_scroll) {
                    return;
                }

                let el = this._history_el.current;
                if (!el) {
                    return;
                }

                el.scrollTo(0, el.scrollHeight);
            });
        }
    }

    _command_key_up = (e) => {
        if (e.key == 'Enter') {
            this._send_click(() => { });
        }

        // TODO: On up/down key, redo a command in the history (if command is empty).
        // TODO: Also support clicking on a history command to copy it.
    }

    _send_click = async (done) => {
        if (this.state._sending_command) {
            return;
        }

        this.setState({ _sending_command: true });

        try {
            // TODO: Need this to have a timeout.
            let res = await this.props.context.channel.call('cnc.Monitor', 'RunMachineCommand', {
                machine_id: this.props.machine.id,
                send_serial_command: this.state._command
            });

            if (!res.status.ok()) {
                throw res.status.toString();
            }

            // NOTE: Command only cleared on 
            this.setState({ _command: '' });

        } catch (e) {
            this.props.context.notifications.add({
                text: 'Send failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        this.setState({ _sending_command: false });
        done();
    }

    render() {
        return (
            <div>
                <div className="card" style={{ overflow: 'hidden' }}>
                    <div className="card-header">
                        History
                    </div>
                    <div ref={this._history_el} style={{ height: 500, fontFamily: 'Noto Sans Mono', fontSize: '0.8em', overflowX: 'hidden', overflowY: 'scroll' }}>
                        {this.state._lines.map((line, i) => {
                            let last = i == this.state._lines.length - 1;

                            // TODO: Do this filtering server side?
                            let kind = line.kind;
                            if (kind == 'OK' && !this.state._show_ok_lines) {
                                return;
                            }

                            if (kind == 'STATE_UPDATE' && !this.state._show_state_lines) {
                                return;
                            }

                            let number = line.number || 0;

                            let number_str = '00000' + number;
                            number_str = number_str.slice(number_str.length - 5);

                            let padding = 5;
                            return (
                                <div key={number} style={{ borderBottom: (last ? null : '1px solid #ccc'), display: 'flex' }}>
                                    <div style={{ padding: padding, backgroundColor: '#eee' }}>
                                        {number_str}
                                    </div>
                                    <div style={{ padding: 5, flexShrink: 10000, wordBreak: 'break-all', }}>{line.value || ' '}</div>
                                </div>
                            );
                        })}
                    </div>
                </div>

                <div style={{ paddingTop: 10 }}>
                    <div className="card">
                        <div className="card-body">
                            <LabeledCheckbox checked={this.state._auto_scroll} onChange={(v) => this.setState({ _auto_scroll: v })}>
                                Auto Scroll
                            </LabeledCheckbox>
                            <LabeledCheckbox checked={this.state._show_ok_lines} onChange={(v) => this.setState({ _show_ok_lines: v })}>
                                Show OK lines
                            </LabeledCheckbox>
                            <LabeledCheckbox checked={this.state._show_state_lines} onChange={(v) => this.setState({ _show_state_lines: v })}>
                                Show state updates
                            </LabeledCheckbox>
                            {/*
                            TODO: Support disabling live updates.
                            */}
                        </div>
                    </div>

                </div>

                <div style={{ paddingTop: 10 }}>
                    <div className="input-group mb-3">
                        <input type="text" className="form-control" placeholder="GCode / command line to send."
                            onKeyUp={this._command_key_up}
                            value={this.state._command}
                            onChange={(e) => this.setState({ _command: e.target.value })}
                        />
                        <Button preset="primary" onClick={this._send_click} spin={this.state._sending_command}>
                            Send
                        </Button>
                    </div>
                </div>
            </div>
        )
    }
}

