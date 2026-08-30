import { StreamingResponseState, Status } from "pkg/web/lib/rpc";
import { WebSocketConnection } from "../websocket";
import { ChannelMessageBased } from "./message";

let INSTANCE = null;

export class ChannelWebSocket {
    static global() {
        if (!INSTANCE) {
            INSTANCE = new ChannelWebSocket();
        }

        return INSTANCE;
    }

    _base: ChannelMessageBased;

    constructor() {
        let sock = WebSocketConnection.global();
        this._base = new ChannelMessageBased((data) => sock.send(JSON.stringify(data)));
        sock.add_string_handler(this._base.handle_message);
    }

    call_streaming(
        service_name: String,
        method_name: String,
        request: any,
        state: StreamingResponseState,
        abort_signal: AbortSignal
    ) {
        return this._base.call_streaming(service_name, method_name, request, state, abort_signal);
    }
}
