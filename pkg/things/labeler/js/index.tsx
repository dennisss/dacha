import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";
import { deep_copy } from "pkg/web/lib/utils";
import { SpinnerInline } from "pkg/web/lib/spinner";
import { Button } from "pkg/web/lib/button";
import { round_digits } from "pkg/web/lib/formatting";

interface AppState {
    devices: any[] | null;

    selected_device_id: string;

    page_input: PageInput;

    preview_data: PreviewData | null;
}

interface PageInput {
    // Incremented by one each time the input is edited.
    revision: number;

    pages: any[];
}

interface PreviewData {
    revision: number;
    page_images: any[];
}

class App extends React.Component<{}, AppState> {

    state: AppState = {
        devices: null,
        selected_device_id: '',
        page_input: this._default_page_input(1),
        preview_data: null
    };

    _last_revision: number = 1;
    _channel: Channel = new Channel('/rpc');

    // If true, we are currently fetching updated preview data for the pages.
    _fetching_preview: boolean = false;

    constructor(props: {}) {
        super(props);

        this._channel.call('labeler.Labeler', 'ListDevices', {}).then((res) => {
            let devices = res.responses[0].devices || [];
            let selected_device_id = '';
            if (devices.length > 0) {
                selected_device_id = devices[0].id;
            }

            this.setState({ devices: res.responses[0].devices, selected_device_id: selected_device_id }, () => {
                this._fetch_previews();
            });
        });
    }

    _fetch_previews() {
        if (this._fetching_preview) {
            return;
        }

        // TODO: If there are many pages, only refresh the preview of the ones that have a change.

        // Stop if already up to date.
        if (this.state.preview_data && this.state.preview_data.revision == this.state.page_input.revision) {
            return;
        }

        if (!this.state.selected_device_id) {
            return;
        }

        this._fetching_preview = true;

        // TODO: Wait 100ms for the revision to stop changing.

        let page_input = deep_copy(this.state.page_input);

        // TODO: Convert mm to dots.

        // TODO: Handle errors with an eventual backoff retry.
        this._channel.call('labeler.Labeler', 'Print', {
            dry_run: true,
            device_id: this.state.selected_device_id,
            pages: page_input.pages
        }).then((res) => {

            // TODO: Also update the device.

            this.setState({
                preview_data: {
                    revision: page_input.revision,
                    page_images: res.responses[0].page_images
                }
            }, () => {
                this._fetching_preview = false;
                this._fetch_previews();
            });
        });
    }

    _print = async (done) => {
        // TODO: Need to stop the print if the tape size was changed since we generated the previous

        let page_input = deep_copy(this.state.page_input);

        try {
            let res = await this._channel.call('labeler.Labeler', 'Print', {
                device_id: this.state.selected_device_id,
                pages: page_input.pages
            });

            // TODO: check status code.

        } catch (e) {
            console.error(e);
        }

        done();
    }

    _default_page_input(revision: number) {
        return {
            revision,
            pages: [{
                text: {
                    value: '',
                    font_size_mm: 0
                },
                quantity: 1,
            }]
        }
    }

    _render_pages(selected_device: any) {
        let pages = this.state.page_input.pages;

        return (
            <div>
                <div>
                    {pages.map((page, i) => {
                        return this._render_single_page(page, i, selected_device);
                    })}
                </div>

                <div style={{ display: "flex", paddingTop: 20 }}>
                    <Button preset="primary" style={{ flexGrow: 1, marginRight: 5 }} onClick={this._print}>Print</Button>
                    <button className="btn btn-light" style={{ flexGrow: 1, border: '1px solid #ccc', marginRight: 5, marginLeft: 5 }} onClick={(e) => {
                        this._update_page_input((page_input) => {
                            page_input.pages.push(this._default_page_input(0).pages[0]);
                        });
                    }}>Add Page</button>
                    <button className="btn btn-danger" style={{ flexGrow: 1, marginLeft: 5 }} onClick={(e) => {
                        this._update_page_input((page_input) => {
                            page_input.pages = this._default_page_input(0).pages;
                        });
                    }}>Clear</button>
                </div>

            </div>



        );

    }

    _update_page_input(f: (v: PageInput) => void, quality_neutral: boolean = false) {
        let page_input = deep_copy(this.state.page_input);

        if (!quality_neutral) {
            this._last_revision += 1;
        }

        page_input.revision = this._last_revision;

        f(page_input);

        this.setState({ page_input }, () => {
            this._fetch_previews();
        });
    }

    _render_single_page(page, i, device) {

        let border_size = 1;
        let horizontal_margin = device.tape.margin || 0 - 2 * border_size;
        let vertical_margin = (device.tape.width - device.tape.print_area) / 2 - 2 * border_size;

        let option_style = { display: 'inline-block', paddingRight: 10, verticalAlign: 'bottom' };

        let [preview_image_el, preview_image] = this._render_preview_image(page, i, device);

        const MM_PER_INCH = 25.4;

        function dots_to_mm(v) {
            return round_digits((v || 0) * (MM_PER_INCH / device.tape.dpi), 1);
        }

        function positive_number(v) {
            v = v * 1;
            if (!v || v < 0) {
                v = 0;
            }

            return v;
        }

        function positive_integer(v) {
            return Math.round(positive_number(v))
        }

        return (
            <div key={i} style={{ paddingBottom: 10 }}>
                <div className="card">
                    <div style={{ backgroundColor: '#eee', borderBottom: '1px solid #ccc', padding: 10, textAlign: 'center' }}>
                        <div className="noscrollbar" style={{ width: '100%', overflowX: 'scroll' }}>
                            {/* This div represents the outer extent of the label paper  */}
                            <div className="label-outer" style={{ backgroundColor: '#fff', paddingLeft: horizontal_margin, paddingRight: horizontal_margin, margin: '0 auto', display: 'inline-block', paddingBottom: vertical_margin, paddingTop: vertical_margin, boxShadow: '0px 10px 15px -3px rgba(0,0,0,0.1)' }}>

                                {/* This div represents the printable area of the label (+ a thin border) */}
                                <div style={{ border: border_size + 'px dashed #ccc' }}>
                                    {preview_image_el}
                                </div>
                            </div>
                        </div>

                        <div style={{ fontSize: '0.8em' }}>
                            {preview_image ? (
                                `Width: ${dots_to_mm(device.tape.width)}mm; Length: ${dots_to_mm(preview_image.width + 2 * device.tape.margin)}mm`
                            ) : '-'}
                        </div>

                    </div>

                    <div className="card-body">
                        <div className="form-floating" style={{ paddingBottom: 10 }}>
                            <textarea className="form-control" style={{ minHeight: 100 }} placeholder="" value={page.text.value} onChange={(e) => {
                                this._update_page_input((page_input) => {
                                    page_input.pages[i].text.value = e.target.value;
                                });
                            }} />
                            <label>Text</label>
                        </div>

                        {page.datamatrix ? (
                            <div className="form-floating" style={{ paddingBottom: 10 }}>
                                <input type="text" className="form-control" placeholder="" value={page.datamatrix.data} onChange={(e) => {
                                    this._update_page_input((page_input) => {
                                        page_input.pages[i].datamatrix.data = e.target.value;
                                    });
                                }} />
                                <label>Datamatrix Value</label>
                            </div>

                        ) : null}

                        <div>
                            <div style={option_style}>
                                <div style={{ fontSize: '0.8em' }}>Quantity</div>
                                <input style={{ width: 100 }} type="number" className="form-control" value={page.quantity} onChange={(e) => {
                                    this._update_page_input((page_input) => {
                                        page_input.pages[i].quantity = positive_integer(e.target.value);
                                    }, true);
                                }} />
                            </div>

                            <div style={option_style}>
                                <div style={{ fontSize: '0.8em' }}>Length (mm)</div>
                                <input style={{ width: 100 }} type="number" className="form-control" value={page.length_mm || 0} onChange={(e) => {
                                    this._update_page_input((page_input) => {
                                        page_input.pages[i].length_mm = positive_number(e.target.value);
                                    });
                                }} />
                            </div>

                            <div style={option_style}>
                                <div style={{ fontSize: '0.8em' }}>Font Size (mm)</div>
                                <input style={{ width: 100 }} type="number" className="form-control" value={page.text.font_size_mm} onChange={(e) => {
                                    this._update_page_input((page_input) => {
                                        page_input.pages[i].text.font_size_mm = positive_number(e.target.value);
                                    });
                                }} />
                            </div>

                            <div style={option_style}>
                                <div className="input-group">
                                    <button className={"btn btn-" + (page.datamatrix ? 'dark' : 'light')} style={{ border: '1px solid #ccc' }} onClick={(e) => {
                                        this._update_page_input((page_input) => {
                                            let p = page_input.pages[i];
                                            if (p.datamatrix) {
                                                p.datamatrix = null;
                                            } else {
                                                p.datamatrix = {
                                                    data: '',
                                                    position: 'LEFT_OF_TEXT'
                                                };
                                            }
                                        });

                                    }}>
                                        <span className="material-symbols-fill">qr_code</span>
                                    </button>
                                    {page.datamatrix ? (
                                        <select className="form-control" value={page.datamatrix.position} onChange={(e) => {
                                            this._update_page_input((page_input) => {
                                                page_input.pages[i].datamatrix.position = e.target.value;
                                            });
                                        }}>
                                            <option value="LEFT_OF_TEXT">Left</option>
                                            <option value="RIGHT_OF_TEXT">Right</option>
                                        </select>
                                    ) : null}
                                </div>
                            </div>

                            <div style={option_style}>
                                <button className="btn btn-danger" onClick={(e) => {
                                    this._update_page_input((page_input) => {
                                        page_input.pages.splice(i, 1);
                                    });
                                }}>
                                    <span className="material-symbols-fill">close</span>
                                </button>
                            </div>
                        </div>

                    </div>
                </div>

            </div>
        );
    }

    _render_preview_image(page, i, device) {
        let up_to_date = false;
        let inner_el = null;
        let image = null;

        if (this.state.preview_data && (this.state.preview_data.page_images || []).length > i) {
            up_to_date = this.state.preview_data.revision == this.state.page_input.revision;

            image = this.state.preview_data.page_images[i];

            // Convert URL safe to regular base64.
            let data = image.data.replaceAll('_', '/').replaceAll('-', '+');

            inner_el = (
                <div style={{ height: device.tape.print_area, width: image.width, backgroundImage: `url(data:image/png;base64,${data})`, fontSize: 0, backgroundSize: 'cover', opacity: (up_to_date ? 1 : 0.5) }}></div>
            );
        }

        let el = (
            <div style={{ height: device.tape.print_area, minWidth: 40, position: 'relative' }}>
                {inner_el}
                {!up_to_date ? (
                    <div style={{ position: 'absolute', left: '50%', top: '50%', transform: 'translate(-50%, -50%) scale(1.5)' }}>
                        <SpinnerInline />
                    </div>
                ) : null}

            </div>

        );

        return [
            el,
            image
        ];
    }

    render() {
        let devices = this.state.devices || [];

        let selected_device_id = this.state.selected_device_id;
        let selected_device = devices.find((dev) => dev.id == selected_device_id);

        return (
            <div className="app-outer">
                <div className="container">
                    <div style={{ padding: '10px 0' }}>
                        <div className="form-floating" style={{ paddingBottom: 10 }}>
                            <select value={selected_device_id} className="form-select" onChange={(e) => {
                                this._last_revision += 1;
                                this.setState({
                                    selected_device_id: e.target.value,
                                    page_input: this._default_page_input(this._last_revision)
                                }, () => {
                                    this._fetch_previews();
                                });
                            }}>
                                <option value=""></option>
                                {devices.map((dev) => {
                                    let tape = dev.tape ? dev.tape.name : 'No tape loaded';
                                    return (
                                        <option disabled={!dev.tape} value={dev.id} key={dev.id}>[{dev.name}] {tape}</option>
                                    );
                                })}
                            </select>
                            <label>Device</label>
                        </div>
                        {selected_device ? this._render_pages(selected_device) : null}
                    </div>
                </div>
            </div>
        );
    }
};




let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)