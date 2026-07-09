// TODO: At some point, dedup all the BlobViewer and MJPEG viewer logic.


import { SpinnerInline } from "pkg/web/lib/spinner";
import React from "react";


export class FrameViewer extends React.Component<{ url: string }> {

    state = {
        transform: { scale: 1, tx: 0, ty: 0 },
        is_dragging: false,
        drag_start: { x: 0, y: 0 },
        img_src: '',
        is_loading: false
    };

    container_ref = React.createRef();


    componentDidMount() {
        this.update_url(this.props.url, this.props.is_live_stream);

        if (this.container_ref.current) {
            // Attach with passive: false to allow e.preventDefault()
            this.container_ref.current.addEventListener('wheel', this.handle_wheel, { passive: false });
        }
    }

    componentDidUpdate(prev_props, prev_state) {
        // Handle URL changes
        if (prev_props.url !== this.props.url || prev_props.is_live_stream !== this.props.is_live_stream) {
            this.update_url(this.props.url, this.props.is_live_stream);
        }

        // Handle global drag event listeners
        if (this.state.is_dragging && !prev_state.is_dragging) {
            window.addEventListener('mousemove', this.handle_mouse_move);
            window.addEventListener('mouseup', this.handle_mouse_up);
        } else if (!this.state.is_dragging && prev_state.is_dragging) {
            window.removeEventListener('mousemove', this.handle_mouse_move);
            window.removeEventListener('mouseup', this.handle_mouse_up);
        }
    }

    componentWillUnmount() {
        if (this.retry_timeout) {
            clearTimeout(this.retry_timeout);
        }
        if (this.container_ref.current) {
            this.container_ref.current.removeEventListener('wheel', this.handle_wheel);
        }
        window.removeEventListener('mousemove', this.handle_mouse_move);
        window.removeEventListener('mouseup', this.handle_mouse_up);
    }

    update_url = (url, is_live_stream) => {
        if (this.retry_timeout) {
            clearTimeout(this.retry_timeout);
            this.retry_timeout = null;
        }

        if (!url) {
            this.setState({ img_src: '', is_loading: false });
            return;
        }

        this.setState({ is_loading: true });
        const src_url = is_live_stream ? `${url}${url.includes('?') ? '&' : '?'}time=${Date.now()}` : url;
        this.setState({ img_src: src_url });
    }

    handle_img_load = () => {
        this.setState({ is_loading: false });
    }

    handle_img_error = () => {
        const { is_live_stream, url } = this.props;
        if (is_live_stream && url) {
            this.retry_timeout = setTimeout(() => {
                const src_url = `${url}${url.includes('?') ? '&' : '?'}time=${Date.now()}`;
                this.setState({ img_src: src_url });
            }, 2000);
        } else {
            this.setState({ is_loading: false });
        }
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
            const new_scale = Math.max(1, Math.min(scale * zoom_factor, 20));
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
                }
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
            }
        }));
    }

    handle_mouse_up = () => {
        if (this.state.is_dragging) {
            this.setState({ is_dragging: false });
        }
    }

    reset_transform = () => {
        this.setState({ transform: { scale: 1, tx: 0, ty: 0 } });
    }

    render() {

        const { flipX = false, flipY = false, controls_enabled = true, points = [] } = this.props;
        const { transform, is_loading, img_src, is_dragging } = this.state;

        const cursor_style = !controls_enabled ? 'default' : is_dragging ? 'grabbing' : 'grab';
        const sx = flipX ? -1 : 1;
        const sy = flipY ? -1 : 1;

        let frame_width = 1920;
        let frame_height = 1200;

        let aspect_ratio = `${frame_width} / ${frame_height}`;

        return (
            <div className="frame-container"
                ref={this.container_ref}
                onMouseDown={this.handle_mouse_down}
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

                    {/* */}
                    <div style={{
                        position: 'absolute', top: 0, left: 0, width: '100%', height: '100%',
                        transformOrigin: 'center center',
                        transform: `scale(${sx}, ${sy})`,
                    }}>
                        <img
                            src={this.props.url}
                            style={{
                                position: 'absolute', width: '100%', height: '100%', top: 0, left: 0,
                                opacity: (is_loading ? 0.5 : 1.0)
                            }}
                            onLoad={this.handle_img_load}
                            onError={this.handle_img_error}
                            onDragStart={(e) => e.preventDefault()}
                        />

                        <svg viewBox="0 0 1920 1200" preserveAspectRatio="xMidYMid meet" style={{
                            width: '100%', height: '100%', top: 0, left: 0, position: 'absolute',
                            pointerEvents: 'none'
                        }}>
                            {points.map((p, i) => (
                                <circle
                                    key={i}
                                    cx={p.x}
                                    cy={p.y}
                                    r={10}
                                    fill="rgba(239, 68, 68, 0.8)"
                                    stroke="#ff0000"
                                    strokeWidth={1}
                                    vectorEffect="non-scaling-stroke"
                                />
                            ))}
                            {points.map((p, i) => (
                                <circle
                                    key={i}
                                    cx={p.x}
                                    cy={p.y}
                                    r={1}
                                    fill="#0bf"
                                    vectorEffect="non-scaling-stroke"
                                />
                            ))}
                        </svg>
                    </div>
                </div>

                {is_loading ? (
                    <div style={{ color: '#fff', position: 'absolute', left: '50%', top: '50%', transform: 'translate(-50%, -50%) scale(1.5)' }}>
                        <SpinnerInline />
                    </div>
                ) : null}

                <button className="btn btn-outline-secondary" onClick={this.reset_transform} style={{ position: 'absolute', bottom: 5, right: 5 }}>
                    Reset
                </button>
            </div>
        );


    }

}
