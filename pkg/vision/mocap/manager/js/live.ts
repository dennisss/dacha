import { Channel } from "pkg/web/lib/rpc";
import { WebSocketConnection } from "pkg/web/lib/websocket";


// This continously pulls image frames for a single camera until it is aborted.
export class CameraLiveFrameSource {
    _camera_id: string;
    _channel: Channel;
    _stream_id: number;
    _abort_controller: AbortController = new AbortController();
    _last_object_url: string | null = null;
    _callback: any;

    constructor(channel, camera_id, callback) {
        this._channel = channel;
        this._camera_id = camera_id;
        this._stream_id = WebSocketConnection.global().create_stream(this._got_data);
        this._callback = callback;

        // Start 
        this._run_request();
    }

    abort() {
        this._gc();
        this._abort_controller.abort();
        WebSocketConnection.global().close_stream(this._stream_id);
    }

    _gc() {
        if (!this._last_object_url) {
            return;
        }

        let url = this._last_object_url;
        setTimeout(() => {
            URL.revokeObjectURL(url);
        }, 100);
        this._last_object_url = null;
    }

    _got_data = (data) => {
        if (this._abort_controller.signal.aborted || this._channel.aborted()) {
            return;
        }

        const blob = new Blob([data], { type: 'image/jpeg' });
        const object_url = URL.createObjectURL(blob);

        this._gc();
        this._last_object_url = object_url;
        (this._callback)(object_url);
    }

    _run_request = async () => {

        while (!this._abort_controller.signal.aborted && !this._channel.aborted()) {

            try {
                let res = await this._channel.call('mocap.Manager', 'ReadFrames', { camera_id: this._camera_id, side_channel_id: this._stream_id }, { abort_signal: this._abort_controller.signal });
                throw new Error('Failed');
            } catch (e) {
                // TODO: Ignore if aborted.
                console.error(e);
            }

            await new Promise((res, rej) => {
                setTimeout(() => {
                    res(null);
                }, 2000);
            })
        }

    }

}
