

let INST: WebSocketConnection | null = null;

export class WebSocketConnection {

    static global(): WebSocketConnection {
        if (!INST) {
            INST = new WebSocketConnection();
        }

        return INST;
    }

    _socket: WebSocket;
    _socket_open: boolean = false;
    _send_queue = [];

    _last_stream_id = 0;
    _handlers: Map<number, any> = new Map();
    _string_handler: any = null;

    constructor() {
        // TODO: Periodically re-open on failures?
        let sock = new WebSocket(`ws://${window.location.host}/`);
        sock.binaryType = 'arraybuffer';

        sock.onopen = () => {
            console.log('WebSocket opened!');
            this._socket_open = true;

            this._send_queue.map((s) => this._socket.send(s));
            this._send_queue = [];
        }
        sock.onerror = (e) => {
            console.error('WebSocket Error:', e);
        }

        sock.onmessage = (e) => {
            if (typeof e.data == 'string') {
                if (this._string_handler) {
                    (this._string_handler)(e.data);
                }
                return;
            }

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

    add_string_handler(f) {
        if (this._string_handler) {
            throw new Error('Multiple string handlers defined');
        }

        this._string_handler = f;
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

    send(data) {
        if (!this._socket_open) {
            this._send_queue.push(data);
            return;
        }

        this._socket.send(data)
    }
}

