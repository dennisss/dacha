import { encode_utf8, decode_utf8, encode_be_u32, decode_be_u32, decode_header_block } from "pkg/web/lib/encoding";

export class Status {
    code: number;
    message?: string;

    constructor(code: number, message?: string) {
        this.code = code;
        this.message = message;
    }

    ok(): boolean {
        return this.code === 0;
    }

    toString(): string {
        return "[" + this.code.toString() + "]" + (this.message ? ": " + this.message : "");
    }
}

export interface RequestOptions {
    abort_signal?: AbortSignal
}

export class StreamingResponse {

    // State object that is shared with the main promise that is reading the response.
    _state: StreamingResponseState

    constructor(state: StreamingResponseState) {
        this._state = state;
    }

    // Blocks for the next message to be received for this request.
    //
    // Returns either a parsed message object or null if the response has ended (either due to an
    // error or the end of the stream). After this returns null, the caller should call finish() to
    // check what the final status of the request is.
    async recv(): Promise<object | null> {
        while (true) {
            if (this._state.response_messages.length == 0) {
                if (this._state.status) {
                    return null;
                }

                await new Promise((res, rej) => {
                    this._state.recv_waiter = res;
                });

                continue;
            }

            let data = this._state.response_messages[0];
            this._state.response_messages.splice(0, 1);

            return data;
        }
    }

    finish(): Status {
        if (!this._state.status) {
            throw new Error('RPC finish() called before all messages were read');
        }

        return this._state.status;
    }

}

class StreamingResponseState {
    recv_waiter: any = null;
    send_waiter: any = null;
    response_messages: object[] = [];
    trailers: Map<String, String> | null = null;
    status: Status | null = null;
}

function status_from_headers(headers: any): Status {
    let code = parseInt(headers.get("grpc-status"));
    if (Number.isNaN(code)) {
        throw new Error("RPC returned invalid status");
    }

    let message = headers.get("grpc-message");

    return new Status(code, message);
}

// TODO: Implement cancellation. Need three types:
// - Deadline based cancellation
// - Voluntary cancellation if a client is done reading a streaming response.
// - Page wide cancellation once we switch to another page.
export class Channel {
    _address: string;
    _abort_signals: AbortSignal[];

    constructor(address: string) {
        this._address = address;
        this._abort_signals = [];
    }

    add_abort_signal(signal: AbortSignal) {
        this._abort_signals.push(signal);
    }

    aborted(): boolean {
        for (let i = 0; i < this._abort_signals.length; i++) {
            if (this._abort_signals[i].aborted) {
                return true;
            }
        }

        return false;
    }

    async call(
        service_name: String, method_name: String, request: any, options: RequestOptions = {}
    ): Promise<{ status: Status, responses: any[], trailers: Map<String, String> }> {

        let res = this.call_streaming(service_name, method_name, request, options);

        let responses = [];
        while (true) {
            let message = await res.recv();
            if (message == null) {
                break;
            }

            responses.push(message);
        }

        let status = res.finish();

        return { status, responses, trailers: res._state.trailers };
    }

    // Calls a service method with a unary request message and returns a streamed response.
    //
    // This should never throw an error. Errors will be propagated back via response.finish().
    call_streaming(service_name: String, method_name: String, request: any, options: RequestOptions = {}): StreamingResponse {
        let state = new StreamingResponseState();

        this._call_streaming_impl(service_name, method_name, request, state, options).catch((e) => {
            state.status = new Status(-1, 'Failed to get response: ' + e);

            if (state.recv_waiter) {
                (state.recv_waiter)();
                state.recv_waiter = null;
            }
        });

        return new StreamingResponse(state);
    }

    async _call_streaming_impl(service_name: String, method_name: String, request: any, state: any, options: RequestOptions) {
        let request_data = encode_utf8(JSON.stringify(request));

        let request_buf = new Uint8Array(1 + 4 + request_data.length);

        request_buf[0] = 0;
        encode_be_u32(request_data.length, new Uint8Array(request_buf.buffer, 1, 4));
        for (let i = 0; i < request_data.length; i++) {
            request_buf[5 + i] = request_data[i];
        }

        let abort_signals = this._abort_signals.slice();
        if (options.abort_signal) {
            abort_signals.push(options.abort_signal);
        }

        // TODO: Configure a default RPC timeout of 30 seconds.
        // This way things always die and must be retried if needed.
        const response = await fetch(`${this._address}/${service_name}/${method_name}`, {
            mode: "cors",
            method: "POST",
            headers: {
                "Content-Type": "application/grpc-web+json"
            },
            body: request_buf,
            credentials: "omit",
            signal: AbortSignal.any(abort_signals)
            // NOTE: Disabling caching on the client side will also break caching of pre-flight requests.
            // cache: "no-cache",
        });

        // Valid gRPC responses should always have a 200 http cod. 
        if (!response.ok) {
            if (response.body) {
                response.body.cancel();
            }

            throw new Error('Request returned a non-OK status: ' + response.status);
        }

        if (!response.body) {
            throw new Error('No body object in response');
        }

        let buffer = new Uint8Array();
        let empty_body = true;

        const reader = response.body.getReader();
        try {
            while (true) {
                let { done, value } = await reader.read();

                if (!value) {
                    value = new Uint8Array();
                }

                if (value.byteLength) {
                    empty_body = false;
                }

                // Append chunk to buffer
                if (buffer.length == 0) {
                    buffer = value;
                } else {
                    buffer = new Uint8Array([...buffer, ...value]);
                }

                // Decode as many messages as we can.
                // i will continue the number of consumed bytes
                let i = 0;
                while (i < buffer.length) {
                    if (i + 5 > buffer.length) {
                        break;
                    }

                    let header = buffer[i];

                    let is_trailers = (header & (1 << 7)) != 0;
                    let compression = header & 0x7f;

                    if (compression != 0) {
                        throw new Error("Response compression not supported");
                    }

                    let data_length = decode_be_u32(new Uint8Array(buffer.buffer, i + 1, 4));
                    if (i + 5 + data_length > buffer.length) {
                        break;
                    }

                    let data = decode_utf8(new Uint8Array(buffer.buffer, i + 5, data_length));
                    i += 5 + data_length;

                    if (is_trailers) {
                        state.trailers = decode_header_block(data);
                        break;
                    } else {
                        state.response_messages.push(JSON.parse(data));

                        if (state.recv_waiter) {
                            (state.recv_waiter)();
                            state.recv_waiter = null;
                        }
                    }
                }

                // Retain all unconsumed data to try again next time.
                buffer = buffer.slice(i);

                if (done) {
                    break;
                }
            }
        } finally {
            await reader.cancel();
        }

        if (buffer.length != 0) {
            throw new Error("Unused data at end of RPC response");
        }

        let status: Status;
        if (response.headers.has("grpc-status")) {
            if (!empty_body) {
                throw new Error("Received a non-empty body in Trailers-Only mode");
            }

            // TODO: Need to remove the grpc status headers from head metadata.
            status = status_from_headers(response.headers);
        } else {
            if (state.trailers === null) {
                throw new Error("RPC response did not end in trailers");
            }

            // TODO: Need to remove the grpc status headers from the trailers object.
            status = status_from_headers(state.trailers);
        }

        state.status = status;
        if (state.recv_waiter) {
            (state.recv_waiter)();
            state.recv_waiter = null;
        }
    }
}

