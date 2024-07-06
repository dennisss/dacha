
const EVENT_NAMES = [
    "seeking", "seeked", "pause", "play", "timeupdate", "stalled", "waiting", "canplaythrough", "canplay"
]

// Sets up a bunch of video event listeners.
// - Event listeners are automatically cleaned up when the abort signal is triggered.
// - When any of the events is triggered or an abort is triggered, we will call the user provided event callback.
export class VideoEventListener {
    _video: HTMLVideoElement;
    _abort_signal: AbortSignal;
    _callback: ((type: string) => void) | null;

    _loaded = false;

    constructor(video: HTMLVideoElement, abort_signal: AbortSignal, callback: ((type: string) => void) | null) {
        this._video = video;
        this._abort_signal = abort_signal;
        this._callback = callback;

        EVENT_NAMES.map((name) => {
            video.addEventListener(name, this._listener);
        });
        abort_signal.addEventListener('abort', this._listener);
    }

    // Returns whether or not we think that the video will be able to play the next frame. 
    loaded(): boolean {
        return this._loaded
    }

    _listener = (e: Event) => {
        if (this._abort_signal.aborted) {
            EVENT_NAMES.map((name) => {
                this._video.removeEventListener(name, this._listener);
            });
            this._abort_signal.removeEventListener('abort', this._listener);
        }

        if (e.type == 'stalled' || e.type == 'waiting') {
            this._loaded = false;
        }
        if (e.type == 'canplay' || e.type == 'canplaythrough') {
            this._loaded = true;
        }
        if (e.type == 'timeupdate' && !this._video.paused && !this._video.seeking) {
            this._loaded = true;
        }

        this._callback();
    }

}