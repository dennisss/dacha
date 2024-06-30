import { ExponentialBackoff } from "pkg/net/src/backoff";
import { SpinnerInline } from "pkg/web/lib/spinner";
import React from "react";

export interface VideoPlayerProps {
    src: string
    style?: any
}

const FRAGMENT_DURATION = 1.0;

/*
Dealing with live video:
- Timestamp 0 reserved for 

*/

export class VideoPlayer extends React.Component<VideoPlayerProps> {

    state = {
        _loading: true,
        _error: false
    }

    _video_el: React.RefObject<HTMLVideoElement>

    _overall_abort_controller: AbortController = new AbortController();

    _current_abort_controller: AbortController | null = null;
    _current_src: string | null = null;

    constructor(props: VideoPlayerProps) {
        super(props);
        this._video_el = React.createRef();

        document.addEventListener('visibilitychange', this._page_visibility_changes);
    }

    componentDidMount(): void {
        this.componentDidUpdate();
    }

    componentDidUpdate(): void {
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
    }

    _page_visibility_changes = () => {
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

    async _run_video(src: string, current_abort_signal: AbortSignal) {
        let abort_signal: AbortSignal = AbortSignal.any([
            this._overall_abort_controller.signal,
            current_abort_signal
        ]);

        let backoff = new ExponentialBackoff({
            base_duration: 1,
            jitter_duration: 2,
            max_duration: 15,
            cooldown_duration: 30,
            max_num_attempts: 0
        });

        while (true) {
            if (abort_signal.aborted || document.hidden) {
                return;
            }

            // If the last attempt failed, pause the video.
            try {
                if (this._video_el.current) {
                    this._video_el.current.pause();
                }
            } catch (e) { }

            await backoff.start_attempt();

            this.setState({
                _loading: true,
                _error: false
            });

            try {
                await this._run_video_attempt(src, abort_signal, () => {
                    backoff.end_attempt(true);

                    this.setState({
                        _loading: false,
                        _error: false
                    });
                })
            } catch (e) {
                if (!abort_signal.aborted) {
                    console.error(e);
                }
            }

            backoff.end_attempt(false);

            if (this._overall_abort_controller.signal.aborted) {
                return;
            }

            this.setState({
                _loading: false,
                _error: true
            });
        }
    }

    async _run_video_attempt(src: string, abort_signal: AbortSignal, got_chunk: () => void) {
        const video = this._video_el.current;
        if (video === null) {
            return;
        }

        const media_source = new MediaSource();

        const media_source_opened = new Promise((res, rej) => {
            media_source.addEventListener('sourceopen', () => {
                res(null)
            });
        });

        video.src = URL.createObjectURL(media_source);
        await media_source_opened;

        const response = await fetch(src, {
            signal: abort_signal
        });

        if (!response.ok || response.body === null) {
            if (response.body !== null) {
                response.body.cancel();
            }

            return;
        }

        const reader = response.body.getReader();

        /*
        TODO: Cancel the current request if we don't get any data back within 10 seconds.

        */

        try {
            const source_buffer: SourceBuffer = media_source.addSourceBuffer(response.headers.get('Content-Type') || '');

            while (true) {
                const { done, value } = await reader.read();
                if (done) {
                    break;
                }

                let update_ended = new Promise((res, rej) => {
                    let listener = () => {
                        source_buffer.removeEventListener('updateend', listener);
                        res(null);
                    };
                    source_buffer.addEventListener('updateend', listener);
                });

                source_buffer.appendBuffer(value);
                await update_ended;

                let end_time = 0;
                if (source_buffer.buffered.length > 0) {
                    end_time = source_buffer.buffered.end(source_buffer.buffered.length - 1);
                }

                // Make sure we will always seeked to near the end of the stream.
                // TODO: Apply some smoothing to this.
                let target_play_time = end_time - 4 * FRAGMENT_DURATION;
                if (video.currentTime < target_play_time) {
                    video.currentTime = end_time - 2 * FRAGMENT_DURATION;
                }

                // Number of seconds of the buffer ending at the current time that we want to keep around. 
                let buffer_target_duration = Math.min(Math.max(5.0, 4 * FRAGMENT_DURATION), 30);

                if (end_time > buffer_target_duration) {
                    let update_ended = new Promise((res, rej) => {
                        let listener = () => {
                            source_buffer.removeEventListener('updateend', listener);
                            res(null);
                        };
                        source_buffer.addEventListener('updateend', listener);
                    });

                    source_buffer.remove(0, end_time - buffer_target_duration);

                    await update_ended;
                }

                // NOTE: Autoplay on page load is only allowed for muted videos.
                try {
                    video.muted = true;
                    video.play();
                } catch (e) {
                    console.error(e);
                }

                got_chunk();
            }

        } finally {
            await reader.cancel();
        }
    }

    render() {
        // TODO: In the backoff mode, switch the spinner to an 'error' icon.

        // TODO: Should have our own pause (this will need to stop the current attempt).

        return (
            <div style={{ position: 'relative', backgroundColor: '#000', minHeight: 100, ...this.props.style }}>
                <video style={{ display: 'block', width: '100%', opacity: (this.state._loading || this.state._error ? 0.5 : undefined) }} ref={this._video_el}></video>
                {this.state._loading || this.state._error ? (
                    <div style={{ color: '#fff', position: 'absolute', left: '50%', top: '50%', transform: 'translate(-50%, -50%) scale(1.5)' }}>
                        <SpinnerInline />
                    </div>
                ) : null}
            </div>
        );
    }
};
