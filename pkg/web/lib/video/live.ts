import { ExponentialBackoff } from "pkg/net/src/backoff";
import { LiveVideoSourceOptions, VideoSourceKind, VideoSource, VideoSourceOptions, VideoStateChangeHandler } from "./types";
import { BACKOFF_OPTIONS } from "./internal";
import { VideoEventListener } from "./listeners";

const FRAGMENT_DURATION = 1.0;

// TODO: If we are playing a muted video, then it will get auto-paused when going into the background.

export class LiveVideoSource extends VideoSource {

    _options: LiveVideoSourceOptions;

    _video: HTMLVideoElement;

    // Overall abort signal used as the VideoSource destructor.
    _abort_signal: AbortSignal;

    _on_state_change: VideoStateChangeHandler;

    _listeners: VideoEventListener;

    // If non-null, then the video is currently attempting to play. So there is a promise running
    // this._run which is controlled by this signal.
    _attempt_abort_controller: AbortController | null = null;

    // If true, we are currently in an error backoff state.
    _error: boolean = false;

    // Media source we are currently using.
    // Note that since switching the 'src' of the video can temporarily pause it, 
    _media_source: MediaSource | null = null;

    constructor(
        options: LiveVideoSourceOptions,
        video: HTMLVideoElement,
        abort_signal: AbortSignal,
        on_state_change: VideoStateChangeHandler
    ) {
        super();

        this._options = options;
        this._video = video;
        this._abort_signal = abort_signal;
        this._on_state_change = on_state_change
        this._run();

        // Trigger an auto-play.
        // NOTE: Autoplay on page load is only allowed for muted videos.
        try {
            video.muted = true;
            video.play();
        } catch (e) {
            console.error(e);
        }
    }

    /*
    As soon as we get a state update, 
    */

    update(options: VideoSourceOptions): void {
        if (options.kind !== VideoSourceKind.Live) {
            throw new Error('Wrong source kind');
        }

        let old_url = this._options.url;
        this._options = options;

        if (options.url != old_url && this._attempt_abort_controller !== null) {
            this._attempt_abort_controller.abort();
            this._attempt_abort_controller = null;
        }
    }

    play() {
        this._video.play();
    }

    pause() {
        try {
            this._video.pause();
        } catch (e) { console.error(e); }
    }

    seek(time: number): void {
        // Not seekable.
    }

    _emit_state() {
        let video = this._video;

        this._on_state_change({
            paused: video.paused,
            // TODO: Use this same code in the fragmented source.
            seeking: video.seeking || !this._listeners.loaded(),
            current_time: video.currentTime,
            error: this._error
        });
    }

    // Main update loop. This is continously running for the live of the LiveVideoSource.
    async _run() {
        let backoff = new ExponentialBackoff(BACKOFF_OPTIONS);

        let waiter: any = null;

        this._listeners = new VideoEventListener(this._video, this._abort_signal, () => {
            if (this._video.paused && this._attempt_abort_controller !== null) {
                console.log('Cancel on pause');

                this._attempt_abort_controller.abort();
                this._attempt_abort_controller = null;
            }

            this._emit_state();

            if (waiter !== null) {
                waiter();
                waiter = null;
            }
        });

        while (true) {
            if (this._abort_signal.aborted) {
                break;
            }

            // Only perform an attempt if we are currently not paused.
            if (this._video.paused) {
                await new Promise((res, _) => { waiter = res });
                continue;
            }

            await backoff.start_attempt();

            if (this._abort_signal.aborted || this._video.paused) {
                continue;
            }

            // Loading...
            this._emit_state();

            this._attempt_abort_controller = new AbortController();
            let attempt_abort_signal: AbortSignal = AbortSignal.any([
                this._abort_signal,
                this._attempt_abort_controller.signal
            ]);

            try {
                await this._run_attempt(this._options.url, attempt_abort_signal, () => {
                    backoff.end_attempt(true);

                    // Playing...
                    this._error = false;
                    this._emit_state();
                })
            } catch (e) {
                if (!attempt_abort_signal.aborted) {
                    console.error(e);
                }
            }

            let was_aborted = attempt_abort_signal.aborted;
            if (this._attempt_abort_controller !== null) {
                this._attempt_abort_controller.abort();
                this._attempt_abort_controller = null;
            }

            if (was_aborted) {
                // In this case, we assume that _run_attempt terminated with an error due an external abortion (which is usually for a good reason like the video being paused).
                backoff.end_attempt(true);
            } else {
                // If we failed for a reason other than being aborted, then report an error.
                backoff.end_attempt(false);
                this._error = true;
                this._emit_state();
            }
        }
    }

    // Runs a single attempt of querying the source url for the live stream.
    // - This is cancelled by the given abort signal (stored in this._attempt_abort_signal).
    // - It will be aborted if:
    //   - The source has to been destroyed.
    //   - The URL has changed.
    //   - The video has paused.
    async _run_attempt(url: string, abort_signal: AbortSignal, got_chunk: () => void) {
        const video = this._video;

        if (this._media_source == null) {
            const media_source = new MediaSource();

            const media_source_opened = new Promise((res, rej) => {
                media_source.addEventListener('sourceopen', () => {
                    res(null)
                });
            });

            video.src = URL.createObjectURL(media_source);
            await media_source_opened;

            this._media_source = media_source;
        }

        const response = await fetch(url, {
            signal: abort_signal
        });

        if (!response.ok || response.body === null) {
            // TODO: Determine if this is still necessary given the this._run always triggers the abort signal after _run_attempt is done running.
            if (response.body !== null) {
                response.body.cancel();
            }

            return;
        }

        // NOTE: Everything after this point runs in a loop to ensure that we always cancel the body.
        const reader = response.body.getReader();

        /*
        TODO: Cancel the current request if we don't get any data back within 10 seconds.
        */

        try {
            let mime_type = response.headers.get('Content-Type') || '';

            if (this._media_source.sourceBuffers.length == 0) {
                this._media_source.addSourceBuffer(mime_type);
            }

            let source_buffer = this._media_source.sourceBuffers[0];

            // Wipe out any old state from previous attempts..
            source_buffer.abort();
            while (source_buffer.buffered.length > 0) {
                let update_ended = new Promise((res, rej) => {
                    let listener = () => {
                        source_buffer.removeEventListener('updateend', listener);
                        res(null);
                    };
                    source_buffer.addEventListener('updateend', listener);
                });

                source_buffer.remove(source_buffer.buffered.start(0), source_buffer.buffered.end(0));

                await update_ended;
            }
            source_buffer.changeType(mime_type);

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
                // The currentTime may also be beyond the end of the buffer if a previous attempt buffered more data than we have right now.
                // TODO: Apply some smoothing to this.
                let target_play_time = end_time - 4 * FRAGMENT_DURATION;
                if (video.currentTime < target_play_time || video.currentTime > end_time) {
                    console.log('CHANGE TIME');
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

                got_chunk();
            }

        } finally {
            // TODO: Determine if this is still necessary given the this._run always triggers the abort signal after _run_attempt is done running.
            await reader.cancel();
        }
    }
}