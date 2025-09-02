import { MJPEGVideoSourceOptions, VideoSourceKind, VideoSource, VideoSourceOptions, VideoStateChangeHandler } from "./types";

export class MJPEGVideoSource extends VideoSource {

    _options: MJPEGVideoSourceOptions;

    _img: HTMLImageElement;

    constructor(
        options: MJPEGVideoSourceOptions,
        video_container: HTMLDivElement,
        abort_signal: AbortSignal,
        on_state_change: VideoStateChangeHandler
    ) {
        super();

        this._options = options;

        let img = document.createElement("img");
        img.src = options.url;
        video_container.appendChild(img);

        on_state_change({
            paused: false,
            seeking: false,
            current_time: 0,
            error: false
        });
    }

    update(options: VideoSourceOptions): void {
        if (options.kind !== VideoSourceKind.MJPEG) {
            throw new Error('Wrong source kind');
        }

        this._options = options;
        this._img.src = options.url;
    }

    play() {
        // Not playable.
    }

    pause() {
        // Not pausable
    }

    seek(time: number): void {
        // Not seekable.
    }
}