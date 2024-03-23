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

    toString(): String {
        return "[" + this.code.toString() + "]" + (this.message ? ": " + this.message : "");
    }
}

function status_from_headers(headers: any): Status {
    let code = parseInt(headers.get("grpc-status"));
    if (Number.isNaN(code)) {
        throw new Error("RPC returned invalid status");
    }

    let message = headers.get("grpc-message");

    return new Status(code, message);
}

export class Channel {
    _address: string;

    constructor(address: string) {
        this._address = address;
    }

    async call(
        service_name: String, method_name: String, request: any
    ): Promise<{ status: any, responses: any[], trailers: Map<String, String> }> {
        let request_data = encode_utf8(JSON.stringify(request));

        let request_buf = new Uint8Array(1 + 4 + request_data.length);

        request_buf[0] = 0;
        encode_be_u32(request_data.length, new Uint8Array(request_buf.buffer, 1, 4));
        for (let i = 0; i < request_data.length; i++) {
            request_buf[5 + i] = request_data[i];
        }

        let res = await fetch(`${this._address}/${service_name}/${method_name}`, {
            mode: "cors",
            method: "POST",
            headers: {
                "Content-Type": "application/grpc-web+json"
            },
            body: request_buf,
            credentials: "omit",
            // NOTE: Disabling caching on the client side will also break caching of pre-flight requests.
            // cache: "no-cache",
        });

        // Valid gRPC responses should always have a 200 http cod. 
        if (!res.ok) {
            throw new Error("RPC returned non-ok status code");
        }

        let raw_buffer = await res.arrayBuffer();

        let buffer = new Uint8Array(raw_buffer);

        // TODO: Support a response with no body and just trailers in the headers.

        let responses = [];
        let trailers = null;

        let i = 0;
        while (i < buffer.length) {

            let header = buffer[i];
            i += 1;

            let is_trailers = (header & (1 << 7)) != 0;
            let compression = header & 0x7f;

            if (compression != 0) {
                throw new Error("Response compression not supported");
            }

            let data_length = decode_be_u32(new Uint8Array(buffer.buffer, i, 4));
            i += 4;

            let data = decode_utf8(new Uint8Array(buffer.buffer, i, data_length));
            i += data_length;

            if (is_trailers) {
                trailers = decode_header_block(data);
                break;
            } else {
                responses.push(JSON.parse(data));
            }
        }

        if (i != buffer.length) {
            throw new Error("Unused data at end of RPC response");
        }

        // TODO: Also extract any header metadata.

        let status;
        if (res.headers.has("grpc-status")) {
            if (buffer.length !== 0) {
                throw new Error("Received a non-empty body in Trailers-Only mode");
            }

            // TODO: Need to remove the grpc status headers from head metadata.
            status = status_from_headers(res.headers);
        } else {
            if (trailers === null) {
                throw new Error("RPC response did not end in trailers");
            }

            // TODO: Need to remove the grpc status headers from the trailers object.
            status = status_from_headers(trailers);
        }

        return {
            status,
            responses,
            trailers
        };
    }
}

