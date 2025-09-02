import React from "react";
import { Button } from "pkg/web/lib/button";
import { PageContext } from "pkg/web/lib/page";
import { Router } from "pkg/web/lib/router";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "./navbar";
import { deep_copy, shallow_copy } from "pkg/web/lib/utils";
import { ContentEditableText } from "pkg/web/lib/content_editable_text";

interface PartsPageState {
    _data: any | null,

    // Map of part id to the target value of it.
    _pending_data: any
}

export interface PartsPageProps {
    context: PageContext
}

export class PartsPage extends React.Component<PartsPageProps, PartsPageState> {
    state: PartsPageState = {
        _data: null,
        _pending_data: {}
    }

    constructor(props: PartsPageProps) {
        super(props);

        this.props.context.channel.call('inventory.Inventory', 'QueryEntities', {}).then((res) => {

            if (!res.status.ok()) {
                this.props.context.notifications.add({
                    text: 'Page load failed: ' + res.status.toString(),
                    cancellable: true,
                    preset: 'danger'
                })
                // throw res.status.toString();
                return;
            }

            let data = res.responses[0];

            let pending_data = {};
            (data.parts || []).map((p) => {
                pending_data[p.id] = p;
            });

            this.setState({
                _data: data,
                _pending_data: pending_data
            });
        });
    }

    _save = async (done) => {
        try {
            let req = { update_parts: [] };

            (this.state._data.parts || []).map((part) => {
                let pending_part = this.state._pending_data[part.id];
                let modified = JSON.stringify(part) !== JSON.stringify(pending_part);
                if (modified) {
                    req.update_parts.push(pending_part);
                }
            });

            let res = await this.props.context.channel.call('inventory.Inventory', 'UpdateEntities', req);
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            let new_data = {
                parts: []
            };

            // TODO: Use the data from the response.
            (this.state._data.parts || []).map((part) => {
                let pending_part = this.state._pending_data[part.id];
                new_data.parts.push(pending_part);
            });

            this.setState({ _data: new_data });

        } catch (e) {
            this.props.context.notifications.add({
                text: 'Save failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        done()
    }

    render() {
        let data = this.state._data || {};

        let parts = data.parts || [];
        let pending_parts = this.state._pending_data;

        let ctx = this.props.context;

        // TODO: Need some sorting capabilities and a way to deal with very long lists.
        // (by default sort by recently added ones).

        let num_modified_parts = 0;

        let part_rows = [];

        parts.map((part) => {
            let pending_part = pending_parts[part.id];
            let modified = JSON.stringify(pending_part) !== JSON.stringify(part);

            if (modified) {
                num_modified_parts += 1;
            }

            part_rows.push(
                <tr key={part.id} className={modified ? 'table-info' : ''}>
                    <td style={{ whiteSpace: 'nowrap', width: 1 }}>
                        {part.id}
                    </td>
                    <td>
                        <ContentEditableText value={pending_part.name} style={{ margin: '0 -4px', padding: '0 4px' }}
                            onChange={(v) => {
                                let new_parts = shallow_copy(pending_parts);
                                let new_part = deep_copy(pending_part);
                                new_part.name = v;
                                new_parts[part.id] = new_part;
                                this.setState({ _pending_data: new_parts });
                            }}
                        />
                    </td>
                    <td>
                        {/* TODO: Would be nice to have a hover hint */}
                        <a href={"https://www.mcmaster.com/catalog/" + part.source.mcmaster_part_number}>
                            <div style={{ fontSize: '0.8em' }}>
                                McMaster<br />{part.source.mcmaster_part_number}
                            </div>
                        </a>
                    </td>
                    <td>
                        x
                    </td>
                </tr>
            );
        });

        return (
            <div>
                <Title value="Parts" />
                <Navbar />

                {/* 60px at the bottom is for the bottom action bar */}
                <div className="container" style={{ paddingTop: 20, paddingBottom: 20, position: 'relative', marginBottom: 60 }}>

                    <table className="table table-hover">
                        <thead>
                            <tr>
                                {/* TODO: Replace with the image */}
                                <th>Id</th>

                                <th>Name</th>
                                <th>Source</th>
                                <th></th>
                            </tr>
                        </thead>
                        <tbody>
                            {part_rows}
                        </tbody>
                    </table>
                </div>

                {num_modified_parts != 0 ? (
                    <div style={{ backgroundColor: '#444', color: '#fff', position: 'fixed', bottom: 0, left: 0, right: 0, padding: '10px 0' }}>
                        <div className="container" style={{ textAlign: 'right' }}>
                            <Button type="submit" preset="primary" onClick={this._save}>Save {num_modified_parts} parts</Button>
                        </div>
                    </div>
                ) : null}
            </div>
        );

    }
}




