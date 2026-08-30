
import { StreamingResponseState, Status } from "pkg/web/lib/rpc";

interface ActiveRequest {
    path: string,
    state: StreamingResponseState,
    abort_signal: AbortSignal,
    abort_listener: any,
    start_time: Date
}

// NOTE: Exactly one instance of this should exist so use ChannelWebView.global() to get it.
export class ChannelMessageBased {
    // TODO: If the page ever ends up getting unloaded, then this will end up getting messed up
    // and out of sync with the server (if using ChannelWebview which persists server state on reload).
    _last_request_id: number = 0;

    _active_requests: Map<number, ActiveRequest> = new Map();

    _send_message: (data: any) => void;

    constructor(send_message) {
        this._send_message = send_message;
    }

    handle_message = (raw_msg) => {
        let msg = JSON.parse(raw_msg);

        if (msg.rpc_response) {
            if (!this._active_requests.has(msg.rpc_response.request_id)) {
                return;
            }

            let req = this._active_requests.get(msg.rpc_response.request_id);
            if (msg.rpc_response.data) {
                req.state.response_messages.push(msg.rpc_response.data);
            }

            if (msg.rpc_response.status) {
                let s = msg.rpc_response.status;
                req.state.status = new Status(s.code, s.message);

                // Cleaning up.
                req.abort_signal.removeEventListener('abort', req.abort_listener);
                this._active_requests.delete(msg.rpc_response.request_id);

                let t = new Date();
                // console.log('RPC Latency ' + req.path, t.getTime() - req.start_time.getTime());
            }

            if (req.state.recv_waiter) {
                (req.state.recv_waiter)();
                req.state.recv_waiter = null;
            }
        }
    }

    call_streaming(
        service_name: String,
        method_name: String,
        request: any,
        state: StreamingResponseState,
        abort_signal: AbortSignal
    ) {
        // TODO: Verify there is no overflow risk.
        let request_id = this._last_request_id + 1;
        this._last_request_id = request_id;

        let start_time = new Date();

        this._send_message({
            start_rpc: {
                service_name,
                method_name,
                request: JSON.stringify(request),
                request_id
            }
        });

        let abort_listener = () => {
            if (!this._active_requests.has(request_id)) {
                return;
            }

            this._active_requests.delete(request_id);

            this._send_message({
                cancel_rpc: { request_id }
            });
        };

        abort_signal.addEventListener('abort', abort_listener, { once: true })

        this._active_requests.set(request_id, {
            path: '/' + service_name + '/' + method_name,
            state,
            abort_listener,
            abort_signal,
            start_time
        });
    }
}