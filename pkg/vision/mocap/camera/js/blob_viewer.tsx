import React from "react";


export class Blob2dViewer extends React.Component<{ status: any, results: any, upsideDown: boolean }> {

    _canvas: React.RefObject<HTMLCanvasElement> = React.createRef();

    state = {
        canvas_width: undefined,
        canvas_height: undefined,
    }

    componentDidMount() {
        this.componentDidUpdate();
    }

    componentDidUpdate() {
        let canvas = this._canvas.current;
        if (!canvas) {
            return;
        }

        let container = canvas.parentElement;
        if (!container) {
            return;
        }

        let status = this.props.status;

        let rect = container.getBoundingClientRect();
        let canvas_width = Math.ceil(rect.width);
        let canvas_height = Math.round((status.sensor.frame_height / status.sensor.frame_width) * canvas_width);

        if (canvas_width != this.state.canvas_width) {
            this.setState({
                canvas_height,
                canvas_width
            });
            return;
        }

        let ctx = canvas.getContext('2d');
        if (!ctx) {
            return;
        }

        ctx.fillStyle = '#000';
        ctx.fillRect(0, 0, canvas_width, canvas_height);

        if (!this.props.results) {
            return;
        }

        let blobs = this.props.results.results.blobs || [];

        let scale = canvas_width / status.sensor.frame_width;

        blobs.map((blob) => {

            // Note that we draw it from the user perspective looking at the camera from in front of it.
            let x = blob.x;
            let y = blob.y;
            if (this.props.upsideDown) {
                y = status.sensor.frame_height - y;
            } else {
                x = status.sensor.frame_width - x;
            }

            ctx.fillStyle = '#fff';
            ctx.beginPath();
            // TODO: Verify 0.5, 0.5 is the center of a pixel.
            ctx.arc(x * scale, y * scale, blob.radius * scale, 0, 2 * Math.PI);
            ctx.fill();
        });
    }

    render() {
        return (
            <div style={{ width: '100%' }}>
                <canvas ref={this._canvas} width={this.state.canvas_width} height={this.state.canvas_height}></canvas>
            </div>
        )
    }
}
