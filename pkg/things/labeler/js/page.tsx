import React from "react";
import { SpinnerInline } from "pkg/web/lib/spinner";
import { round_digits } from "pkg/web/lib/formatting";


export interface PageImageComponentProps {
    device: any,

    // The PageImage protobuf.
    data: any | null,

    // True if the data represents a stale view of the user's inputs (still generating a new image). 
    dirty: boolean
}

export class PageImageWrapperComponent extends React.Component<PageImageComponentProps> {
    render() {
        const MM_PER_INCH = 25.4;

        let device = this.props.device;

        function dots_to_mm(v) {
            return round_digits((v || 0) * (MM_PER_INCH / device.tape.dpi), 1);
        }

        return (
            <div style={{ backgroundColor: '#eee', padding: 10, textAlign: 'center' }}>
                <div className="noscrollbar" style={{ width: '100%', overflowX: 'scroll' }}>
                    {/* This div represents the outer extent of the label paper  */}
                    <PageImageComponent device={device} data={this.props.data} dirty={this.props.dirty} />
                </div>

                <div style={{ fontSize: '0.8em' }}>
                    {this.props.data ? (
                        `Width: ${dots_to_mm(device.tape.width)}mm; Length: ${dots_to_mm(this.props.data.width + 2 * device.tape.margin)}mm`
                    ) : '-'}
                </div>
            </div>
        );
    }
}

// This component renders a single label page/strip. The bounding box of this component
// corresponds to the physical edge of the label paper. 
export class PageImageComponent extends React.Component<PageImageComponentProps> {

    _render_preview_image() {
        let device = this.props.device;

        let inner_el = null;
        if (this.props.data) {
            // Convert URL safe to regular base64.
            let data = this.props.data.data.replaceAll('_', '/').replaceAll('-', '+');

            inner_el = (
                <div style={{ height: device.tape.print_area, width: this.props.data.width, backgroundImage: `url(data:image/png;base64,${data})`, fontSize: 0, backgroundSize: 'cover', opacity: (this.props.dirty ? 0.5 : 1) }}></div>
            );
        }

        return (
            <div style={{ height: device.tape.print_area, minWidth: 40, position: 'relative' }}>
                {inner_el}
                {this.props.dirty ? (
                    <div style={{ position: 'absolute', left: '50%', top: '50%', transform: 'translate(-50%, -50%) scale(1.5)' }}>
                        <SpinnerInline />
                    </div>
                ) : null}
            </div>
        );
    }

    render() {
        let device = this.props.device;

        let border_size = 1;
        let horizontal_margin = device.tape.margin || 0 - 2 * border_size;
        let vertical_margin = (device.tape.width - device.tape.print_area) / 2 - 2 * border_size;

        let preview_image_el = this._render_preview_image();

        return (
            <div className="label-outer" style={{ backgroundColor: '#fff', paddingLeft: horizontal_margin, paddingRight: horizontal_margin, margin: '0 auto', display: 'inline-block', paddingBottom: vertical_margin, paddingTop: vertical_margin, boxShadow: '0px 10px 15px -3px rgba(0,0,0,0.1)' }}>

                {/* This div represents the printable area of the label (+ a thin border) */}
                <div style={{ border: border_size + 'px dashed #ccc' }}>
                    {preview_image_el}
                </div>
            </div>
        );
    }
};
