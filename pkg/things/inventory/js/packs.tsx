import React from "react";
import { Button } from "pkg/web/lib/button";
import { PageContext } from "pkg/web/lib/page";
import { Router } from "pkg/web/lib/router";
import { Title } from "pkg/web/lib/title";
import { Navbar } from "./navbar";
import { deep_copy, shallow_copy } from "pkg/web/lib/utils";
import { ContentEditableText } from "pkg/web/lib/content_editable_text";
import { PageImageWrapperComponent } from "pkg/things/labeler/js/page";

interface PacksPageState {
    _data: any | null,

    // Map of part id to the target value of it.
    _pending_data: any,

    // Map of pack ids which we want to print.
    _printing: any,
}

export interface PacksPageProps {
    context: PageContext
}

export class PacksPage extends React.Component<PacksPageProps, PacksPageState> {
    state: PacksPageState = {
        _data: null,
        _pending_data: {},
        _printing: {}
    }

    constructor(props: PacksPageProps) {
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
            (data.packs || []).map((p) => {
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
            let req = { update_packs: [] };

            (this.state._data.packs || []).map((pack) => {
                let pending_pack = this.state._pending_data[pack.id];
                let modified = JSON.stringify(pack) !== JSON.stringify(pending_pack);
                if (modified) {
                    req.update_packs.push(pending_pack);
                }
            });

            let res = await this.props.context.channel.call('inventory.Inventory', 'UpdateEntities', req);
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            let new_data = {
                packs: []
            };

            // TODO: Use the data from the response.
            (this.state._data.packs || []).map((pack) => {
                let pending_pack = this.state._pending_data[pack.id];
                new_data.packs.push(pending_pack);
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

    _print = async (done) => {
        try {
            let req = { pack_ids: [] };

            (this.state._data.packs || []).map((pack) => {
                if (this.state._printing[pack.id] === true) {
                    req.pack_ids.push(pack.id);
                }
            });

            let res = await this.props.context.channel.call('inventory.Inventory', 'PrintLabels', req);
            if (!res.status.ok()) {
                throw res.status.toString();
            }

            this.setState({ _printing: {} });

        } catch (e) {
            this.props.context.notifications.add({
                text: 'Printing failed: ' + e,
                cancellable: true,
                preset: 'danger'
            });
        }

        done()
    }

    render() {
        let data = this.state._data || {};

        let parts = data.parts || [];
        let parts_map = {}
        parts.map((p) => {
            parts_map[p.id] = p;
        })


        let packs = data.packs || [];
        let pending_packs = this.state._pending_data;

        console.log(parts_map);

        let ctx = this.props.context;

        // TODO: Need some sorting capabilities and a way to deal with very long lists.
        // (by default sort by recently added ones).

        let num_modified_packs = 0;
        let num_printing = 0;

        let pack_rows = [];

        packs.map((pack) => {
            let pending_pack = pending_packs[pack.id];
            let modified = JSON.stringify(pending_pack) !== JSON.stringify(pack);

            let part = parts_map[pending_pack.part_id];
            console.log(pending_pack.part_id, part);

            if (modified) {
                num_modified_packs += 1;
            }

            let printing = this.state._printing[pack.id] || false;

            pack_rows.push(
                <tr key={pack.id} className={modified ? 'table-info' : ''}>
                    <td style={{ whiteSpace: 'nowrap', width: 1 }}>
                        {pack.id}
                    </td>
                    <td>
                        <div style={{ whiteSpace: 'pre' }}>
                            {part.name}
                        </div>
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
                        <button className={"btn btn-" + (printing ? 'dark' : 'light')} style={{ border: '1px solid #ccc' }} onClick={(e) => {
                            let p = shallow_copy(this.state._printing);
                            p[pack.id] = !printing;

                            this.setState({
                                _printing: p
                            });
                        }}>
                            <span className="material-symbols-outlined">
                                print
                            </span>
                        </button>
                    </td>
                </tr>
            );

            if (printing) {
                num_printing += 1;

                pack_rows.push(
                    <tr key={pack.id + '-label'}>
                        <td colSpan={4} style={{ padding: 0 }}>
                            <PackLabel context={this.props.context} pack={pack} />
                        </td>
                    </tr>
                );
            }
        });

        return (
            <div>
                <Title value="Packs" />
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
                            {pack_rows}
                        </tbody>
                    </table>
                </div>

                {num_modified_packs != 0 || num_printing != 0 ? (
                    <div style={{ backgroundColor: '#444', color: '#fff', position: 'fixed', bottom: 0, left: 0, right: 0, padding: '10px 0' }}>
                        <div className="container" style={{ textAlign: 'right' }}>
                            {num_modified_packs != 0 ? (
                                <Button type="submit" preset="primary" onClick={this._save} style={{ marginLeft: 10 }}>
                                    Save {num_modified_packs} packs
                                </Button>
                            ) : null}
                            {num_printing != 0 ? (
                                <Button type="submit" preset="primary" onClick={this._print} style={{ marginLeft: 10 }}>
                                    Print {num_printing} labels
                                </Button>
                            ) : null}


                        </div>
                    </div>
                ) : null}
            </div>
        );

    }
}

interface PackLabelProps {
    pack: any,
    context: PageContext
}

class PackLabel extends React.Component<PackLabelProps> {

    state = {
        _data: null
    }

    constructor(props: PackLabelProps) {
        super(props);

        // TODO: Make this refresh if the name changes.
        this.props.context.channel.call('inventory.Inventory', 'PrintLabels', {
            dry_run: true,
            pack_ids: [this.props.pack.id]
        }).then((res) => {

            if (!res.status.ok()) {
                throw res.status.toString();
            }

            this.setState({ _data: res.responses[0].labels[0] });
        })
    }

    render() {

        if (!this.state._data) {
            return <div>
                ...
            </div>
        };

        console.log(this.state._data);

        return (
            <PageImageWrapperComponent
                dirty={false} device={this.state._data.device}
                data={this.state._data.page_images[0]} />
        );
    }

}




