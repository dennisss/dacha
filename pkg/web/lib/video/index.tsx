import React from "react";
import { SpinnerInline } from "../spinner";
import { VideoControls } from "./controls";
import { VideoSourceKind, VideoSource, VideoSourceOptions, VideoState } from "./types";
import { FragmentedVideoSource } from "./fragmented";
import { LiveVideoSource } from "./live";

/*

let v = document.getElementsByTagName('video')[0];
v.addEventListener("seeking", (event) => console.log('SEEKING'));
v.addEventListener("seeked", (event) => console.log('SEEKED'));
v.addEventListener("pause", (event) => console.log('PAUSE'));
v.addEventListener("play", (event) => console.log('PLAY'));

v.currentTime = 1719952609.197
v.play();

v.currentTime = 1719952607.197


v.currentTime = 1719955198.0226445;
v.play()

*/


// Time in milliseconds after which we will hide the controls if we are not playing and see no mouse movement.
const ACTIVE_TIMEOUT: number = 5000;

export interface VideoPlayerProps {
    source: VideoSourceOptions;

    style?: any;

    onStateChange?: (state: VideoState) => void;
}

export class VideoPlayer extends React.Component<VideoPlayerProps, { _state: VideoState, _active: boolean }> {

    state = {
        _state: {
            seeking: true,
            paused: true,
            error: false,
            current_time: 0,
        },
        _active: false
    }

    _container: React.RefObject<HTMLDivElement> = React.createRef();

    _video_el: React.RefObject<HTMLVideoElement>;
    _video_source: VideoSource | null = null;


    _overall_abort_controller: AbortController = new AbortController();

    _current_abort_controller: AbortController | null = null;
    _current_src: string | null = null;

    constructor(props: VideoPlayerProps) {
        super(props);
        this._video_el = React.createRef();

        document.addEventListener('visibilitychange', this._page_visibility_changes);
    }

    componentDidMount(): void {
        let video = this._video_el.current;
        if (!video) {
            throw new Error('Missing video element');
        }

        let abort_signal = this._overall_abort_controller.signal;

        let source = this.props.source;

        if (source.kind == VideoSourceKind.Live) {
            this._video_source = new LiveVideoSource(source, video, abort_signal, this._on_source_state_change);
        } else if (source.kind == VideoSourceKind.Fragmented) {
            this._video_source = new FragmentedVideoSource(source, video, abort_signal, this._on_source_state_change);
        }

        return;

        this.componentDidUpdate();
    }

    componentDidUpdate(): void {
        return;

        if (this._current_src === this.props.src || document.hidden) {
            return;
        }

        if (this._current_abort_controller !== null) {
            this._current_abort_controller.abort();
        }

        this._current_src = this.props.src;
        this._current_abort_controller = new AbortController();
        this._run_video(this._current_src, this._current_abort_controller.signal);
    }

    componentWillUnmount(): void {
        this._overall_abort_controller.abort();
        document.removeEventListener('visibilitychange', this._page_visibility_changes);

        if (this._active_timeout !== null) {
            clearTimeout(this._active_timeout);
        }
    }

    // TODO: Verify this never gets called in the constructor since we can't change the state yet.
    _on_source_state_change = (state: VideoState) => {
        try {
            this.setState({
                // TODO: Verify that the seeking implementation is robust.
                _state: state
            });

            if (this.props.onStateChange) {
                this.props.onStateChange(state);
            }
        } catch (e) {
            // NOTE: Don't want client exceptions to propagate to the VideoSource code that calls this event handler.
            console.error(e);
        }

    }

    _page_visibility_changes = () => {
        return;

        if (document.hidden) {
            // TODO: Set a timeout on this. If still hidden after 1 second, cancel it.

            if (this._current_abort_controller !== null) {
                this._current_abort_controller.abort();
                this._current_abort_controller = null;
                this._current_src = null;
            }
        } else {
            this.componentDidUpdate();
        }
    }

    _on_play_toggle = () => {
        let state = this.state._state;
        if (state.paused) {
            this._video_source?.play();
        } else {
            this._video_source?.pause();
        }
    }

    _active_timeout: any = null;
    _on_mouse_move = () => {

        this.setState({ _active: true });

        if (this._active_timeout !== null) {
            clearTimeout(this._active_timeout);
        }

        this._active_timeout = setTimeout(() => {
            this._active_timeout = null;
            this.setState({ _active: false });
        }, ACTIVE_TIMEOUT);
    }

    _on_mouse_leave = () => {
        this.setState({ _active: false });
    }

    render() {
        // TODO: In the backoff mode, switch the spinner to an 'error' icon.

        // TODO: Should have our own pause (this will need to stop the current attempt).

        // TODO: Our loading spinner may overlap with the browser's loading spinner for the seeking state.

        let state = this.state._state;

        let in_full_screen = (this._container.current ? true : false) && document.fullscreenElement == this._container.current;

        let active = this.state._active || state.paused;

        let show_loading = (state.paused ? false : state.seeking) || state.error;

        return (
            <div className={"video-player" + (active ? ' active' : '')} ref={this._container} style={{ position: 'relative', backgroundColor: '#000', minHeight: 100, ...this.props.style }}
                onMouseMove={this._on_mouse_move}
                onMouseLeave={this._on_mouse_leave}
            >
                <video style={{ opacity: (show_loading ? 0.5 : undefined) }} ref={this._video_el} onClick={this._on_play_toggle}></video>
                {show_loading ? (
                    <div style={{ color: '#fff', position: 'absolute', left: '50%', top: '50%', transform: 'translate(-50%, -50%) scale(1.5)' }}>
                        <SpinnerInline />
                    </div>
                ) : null}
                <VideoControls
                    state={state}
                    onPlayToggle={this._on_play_toggle}
                    isFullscreen={in_full_screen}
                    onFullscreenToggle={async () => {
                        try {
                            if (in_full_screen) {
                                await document.exitFullscreen();
                            } else {
                                await this._container.current.requestFullscreen();
                            }
                        } catch (e) {
                            console.error(e);
                        }

                        // Re-render with the new setting of in_full_screen.
                        this.forceUpdate();
                    }}
                    onSeek={(time) => {
                        this._video_source?.seek(time);
                    }}

                />
            </div>
        );
    }
};