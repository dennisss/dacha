

let INST: WebSocketConnection | null = null;

export class WebSocketConnection {

    static global(): WebSocketConnection {
        if (!INST) {
            INST = new WebSocketConnection();
        }

        return INST;
    }

    _last_stream_id = 0;
    _handlers: Map<number, any> = new Map();

    constructor() {
        let sock = new WebSocket(`ws://${window.vars.webview_http_server}/`);
        sock.binaryType = 'arraybuffer';

        sock.onopen = () => {
            console.log('WebSocket opened!');
        }
        sock.onerror = (e) => {
            console.error('WebSocket Error:', e);
        }

        sock.onmessage = (e) => {
            const buffer: ArrayBuffer = e.data;

            const data_view = new DataView(buffer);
            const stream_id = data_view.getUint32(0, true);

            const data = new Uint8Array(buffer, 4);

            if (!this._handlers.has(stream_id)) {
                return;
            }

            (this._handlers.get(stream_id))(data);
        }

        this._socket = sock;
    }

    create_stream(callback): number {
        let id = this._last_stream_id + 1;
        this._last_stream_id = id;
        this._handlers.set(id, callback);
        return id;
    }

    close_stream(id) {
        this._handlers.delete(id);
    }
}

