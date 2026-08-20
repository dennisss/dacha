/*
Some implementation notes on this:
- The image needs to be in the <svg> so that Chrome correctly aligns the pixels when zoomed in.
- Using an <img> for reading MJPEG streams seems to be the most efficient way to do this (JS fetch and blob copying seems to be much slower).
    - By unfortunately this doesn't expose any stats on how quickly new images are fetched.
- I tried putting this all in a canvas but its hard to get good performance compared to the current pure SVG solution.
*/

// TODO: At some point, dedup all the BlobViewer and MJPEG viewer logic.


import { SpinnerInline } from "pkg/web/lib/spinner";
import React from "react";
import { CameraLiveFrameSource } from "./live";
import { Channel } from "pkg/web/lib/rpc";

interface Point {
    x: number;
    y: number;
    radius_a: number;
    radius_b: number;
    angle: number;
}

interface FrameViewerProps {
    image_source?: ImageSource;
    controls_enabled: boolean;
    flipX: boolean;
    flipY: boolean;
    points: Point[];
    show_crosshair?: boolean;
    channel?: Channel
}

interface ImageSource {
    url?: string,
    camera_id?: string
}

export class FrameViewer extends React.Component<FrameViewerProps> {

    state = {
        transform: { scale: 1, tx: 0, ty: 0 },
        is_dragging: false,
        drag_start: { x: 0, y: 0 },
        moved: false,

        // This is the current URL we should have the HTML try to render.
        img_src: '',
        // Whether or not we are still loading the next frame (currently stays
        // false after the first frame for a source is loaded).
        is_loading: false,
    };

    container_ref = React.createRef();

    source_inst: CameraLiveFrameSource | null = null;


    componentDidMount() {
        this.update_source(this.props.image_source);

        if (this.container_ref.current) {
            // Attach with passive: false to allow e.preventDefault()
            this.container_ref.current.addEventListener('wheel', this.handle_wheel, { passive: false });
        }
    }

    componentDidUpdate(prev_props, prev_state) {
        // Handle URL changes
        if (JSON.stringify(prev_props.image_source || {}) != JSON.stringify(this.props.image_source || {})) {
            this.update_source(this.props.image_source);
        }

        // Handle global drag event listeners
        if (this.state.is_dragging && !prev_state.is_dragging) {
            window.addEventListener('mousemove', this.handle_mouse_move);
            window.addEventListener('mouseup', this.handle_mouse_up);
        } else if (!this.state.is_dragging && prev_state.is_dragging) {
            window.removeEventListener('mousemove', this.handle_mouse_move);
            window.removeEventListener('mouseup', this.handle_mouse_up);
        }

        // TODO: Technically should use a default value of 'true' if the prop is undefined
        if (!this.props.controls_enabled && prev_props.controls_enabled) {
            this.reset_transform();
        }
    }

    componentWillUnmount() {
        if (this.source_inst) {
            this.source_inst.abort();
            this.source_inst = null;
        }

        if (this.container_ref.current) {
            this.container_ref.current.removeEventListener('wheel', this.handle_wheel);
        }
        window.removeEventListener('mousemove', this.handle_mouse_move);
        window.removeEventListener('mouseup', this.handle_mouse_up);
    }

    update_source = (image_source) => {

        if (this.source_inst) {
            this.source_inst.abort();
            this.source_inst = null;
        }

        image_source = image_source || {};

        if (image_source.url) {
            this.setState({ is_loading: true, img_src: image_source.url });
            return;
        }

        if (image_source.camera_id) {
            this.source_inst = new CameraLiveFrameSource(this.props.channel, image_source.camera_id, (image) => {
                this.setState({ img_src: image });
            });

            // Blank image until we get the first frame.
            this.setState({ img_src: '', is_loading: true });

            return;
        }

        // No image source
        this.setState({ img_src: '', is_loading: false });
    }

    handle_img_load = () => {
        this.setState({ is_loading: false });
    }

    // NOTE: The error handler only works well to detect a single image failure.
    // If the URL ends up being a chunked stream, it can't detect an unexpected end
    // of stream after the first image is loaded. 
    handle_img_error = () => {
        this.setState({ is_loading: false });
    }

    handle_wheel = (e) => {
        const { controls_enabled = true } = this.props;
        if (!controls_enabled) return;
        e.preventDefault(); // Prevent page scroll

        const zoom_speed = 1.1;
        const zoom_factor = e.deltaY < 0 ? zoom_speed : (1 / zoom_speed);
        const rect = this.container_ref.current.getBoundingClientRect();

        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;

        this.setState(prev_state => {
            const { scale, tx, ty } = prev_state.transform;
            const new_scale = Math.max(1, Math.min(scale * zoom_factor, 40));
            const ratio = new_scale / scale;

            return {
                transform: {
                    scale: new_scale,
                    tx: mx - (mx - tx) * ratio,
                    ty: my - (my - ty) * ratio
                }
            };
        });
    }

    handle_mouse_down = (e) => {
        const { controls_enabled = true } = this.props;
        if (!controls_enabled) return;

        if (e.button === 0 || e.button === 1) { // Left or middle click
            const { tx, ty } = this.state.transform;
            this.setState({
                is_dragging: true,
                drag_start: {
                    x: e.clientX - tx,
                    y: e.clientY - ty
                },
                moved: false
            });
        }
    }

    handle_mouse_move = (e) => {
        if (!this.state.is_dragging) return;
        const { drag_start } = this.state;

        this.setState((prev_state) => ({
            transform: {
                ...prev_state.transform,
                tx: e.clientX - drag_start.x,
                ty: e.clientY - drag_start.y
            },
            moved: true
        }));
    }

    handle_mouse_up = (e) => {
        if (this.state.is_dragging) {
            this.setState({ is_dragging: false });
        }

        setTimeout(() => {
            // Cancel this once the onClick event fires.
            this.setState({ moved: false })
        })
    }

    reset_transform = () => {
        this.setState({ transform: { scale: 1, tx: 0, ty: 0 } });
    }

    render() {

        const { flipX = false, flipY = false, controls_enabled = true, points = [], show_crosshair = false } = this.props;
        const { transform, is_loading, img_src, is_dragging } = this.state;

        const cursor_style = !controls_enabled ? null : is_dragging ? 'grabbing' : 'grab';
        const sx = flipX ? -1 : 1;
        const sy = flipY ? -1 : 1;

        // TODO: Make this more dynamic.
        let frame_width = 1920;
        let frame_height = 1200;

        let aspect_ratio = `${frame_width} / ${frame_height}`;

        return (
            <div className="frame-container"
                ref={this.container_ref}
                onMouseDown={this.handle_mouse_down}
                onClick={(e) => {
                    if (this.state.moved) {
                        e.stopPropagation();
                    }
                }}
                style={{
                    aspectRatio: aspect_ratio, width: '100%', backgroundColor: '#666', position: 'relative',
                    overflow: 'hidden',
                    cursor: cursor_style
                }}>

                {/* viewport */}
                <div style={{
                    position: 'absolute', top: 0, left: 0, width: '100%', height: '100%',
                    transformOrigin: '0 0',
                    transform: `translate(${transform.tx}px, ${transform.ty}px) scale(${transform.scale})`
                }}>
                    <div style={{
                        position: 'absolute', top: 0, left: 0, width: '100%', height: '100%',
                        transformOrigin: 'center center',
                        transform: `scale(${sx}, ${sy})`,
                    }}>
                        <svg viewBox="0 0 1920 1200" preserveAspectRatio="none" style={{
                            width: '100%', height: '100%', top: 0, left: 0, position: 'absolute',
                            pointerEvents: 'none'
                        }}>
                            {img_src ? (
                                <image
                                    href={img_src}
                                    width={frame_width}
                                    height={frame_height}
                                    preserveAspectRatio="none"
                                    style={{
                                        opacity: is_loading ? 0.5 : 1.0,
                                        // Pixelation is slow in Chrome due to bad GPU acceleation so only do it when zoomed in.
                                        imageRendering: transform.scale > 2 ? "pixelated" : undefined,
                                    }}
                                    onLoad={this.handle_img_load}
                                    onAbort={this.handle_img_error}
                                    onError={this.handle_img_error}
                                />
                            ) : (
                                /* black background if no image is given */
                                <rect width={frame_width} height={frame_height} fill="#000" />
                            )}

                            {points.map((p, i) => (
                                <ellipse
                                    key={i}
                                    cx={p.x}
                                    cy={p.y}
                                    rx={p.radius_a}
                                    ry={p.radius_b}
                                    transform={`rotate(${(p.angle || 0) * (180 / Math.PI)}, ${p.x}, ${p.y})`}
                                    fill={img_src ? "rgba(239, 68, 68, 0.6)" : '#fff'}
                                    stroke="#ff0000"
                                    strokeWidth={img_src ? 1 / transform.scale : 0}
                                />
                            ))}
                            {img_src ? points.map((p, i) => (
                                <circle
                                    key={i}
                                    cx={p.x}
                                    cy={p.y}
                                    r={1}
                                    fill="#0bf"
                                />
                            )) : null}

                            {show_crosshair && (
                                <g stroke="rgba(255, 255, 255, 0.8)" strokeWidth={1 / transform.scale}>
                                    <line x1={0} y1={frame_height / 2} x2={frame_width} y2={frame_height / 2} />
                                    <line x1={frame_width / 2} y1={0} x2={frame_width / 2} y2={frame_height} />
                                </g>
                            )}
                        </svg>
                    </div>
                </div>

                {is_loading ? (
                    <div style={{ color: '#fff', position: 'absolute', left: '50%', top: '50%', transform: 'translate(-50%, -50%) scale(1.5)' }}>
                        <SpinnerInline />
                    </div>
                ) : null}

                {controls_enabled ? (
                    <button className="btn btn-outline-secondary" onClick={(e) => {
                        this.reset_transform();
                        e.stopPropagation();
                    }} style={{ position: 'absolute', bottom: 5, right: 5 }}>
                        Reset
                    </button>
                ) : null}

            </div>
        );


    }

}
