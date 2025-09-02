import React from "react";
import { PageContext } from "pkg/web/lib/page";
import { MachineUiState } from "./state";
import { Card, CardBody } from "../card";
import { Button } from "pkg/web/lib/button";
import { run_machine_command } from "../rpc_utils";
import { shallow_copy } from "pkg/web/lib/utils";

export interface ObjectsBoxProps {
    machine: any;
    context: PageContext;
    ui_state: MachineUiState;
}

export class ObjectsBox extends React.Component<ObjectsBoxProps> {

    _click_cancel(index, cancelled, done) {
        run_machine_command(this.props.context, this.props.machine, {
            toggle_object: {
                object_index: index,
                cancelled: cancelled
            }
        }, done);
    }

    render() {
        let machine = this.props.machine;

        let objects = machine.state.loaded_program?.file?.program.objects;
        if (!objects) {
            return null;
        }

        let objects_state = machine.state.running_program?.objects;

        let current_index = -1;
        if (objects_state) {
            current_index = objects_state.current_object_index || 0;
        }

        let ui_state = this.props.ui_state;

        return (
            <Card id="objects" header="Objects" style={{ marginBottom: 10 }}>
                <CardBody>
                    <div style={{ wordBreak: 'break-all' }}>
                        <table className="table" style={{ margin: 0 }}>
                            <tbody>
                                {objects.map((obj, i) => {

                                    let legend_entry = ui_state.position_legend().get('object_' + i);;
                                    let highlight = legend_entry ? legend_entry.focused : false;


                                    let is_active = i == current_index;
                                    let is_cancelled = false;
                                    let can_cancel = false;
                                    let can_resume = false;

                                    if (objects_state) {
                                        let state = objects_state.objects[i];
                                        can_cancel = !state.cancelled;
                                        is_cancelled = state.cancelled;
                                        can_resume = state.cancelled && (state.cancelled.skipped_lines || 0) == 0;
                                    }

                                    let mouse_enter = () => {
                                        setTimeout(() => {
                                            if (legend_entry) {
                                                let e = shallow_copy(legend_entry);
                                                e.focused = true;
                                                ui_state.position_legend().set(e);
                                            }
                                        }, 2);
                                    };

                                    let mouse_exit = () => {
                                        if (legend_entry) {
                                            let e = shallow_copy(legend_entry);
                                            e.focused = false;
                                            ui_state.position_legend().set(e);
                                        }
                                    };

                                    return (
                                        <tr key={i} style={{ verticalAlign: 'baseline' }} className={highlight ? 'table-active' : null}
                                            onMouseEnter={mouse_enter} onMouseLeave={mouse_exit}

                                        >
                                            <td>
                                                <div style={{ width: '100%', overflowX: 'hidden', fontSize: '0.8em', fontWeight: (is_active ? 'bold' : null), color: (is_cancelled ? 'red' : '') }}>
                                                    {obj.name} {is_active ? ' (Current)' : ''}
                                                </div>
                                            </td>
                                            <td style={{ width: 1, whiteSpace: 'nowrap', textAlign: 'right' }}>
                                                {can_cancel ? (
                                                    <Button preset="secondary" style={{ width: '100%' }} small={true}
                                                        onClick={(done) => this._click_cancel(i, true, done)}
                                                    >
                                                        Cancel
                                                    </Button>
                                                ) : (can_resume ? (
                                                    <Button preset="primary" style={{ width: '100%' }} small={true}
                                                        onClick={(done) => this._click_cancel(i, false, done)}
                                                    >
                                                        Resume
                                                    </Button>
                                                ) : null)}
                                            </td>
                                        </tr>
                                    );
                                })}
                            </tbody>
                        </table>

                    </div>

                </CardBody>
            </Card>
        );
    }
}