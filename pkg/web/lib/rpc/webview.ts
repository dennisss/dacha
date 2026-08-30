import { StreamingResponseState, Status } from "pkg/web/lib/rpc";
import { ChannelMessageBased } from "./message";

function post_message_webview(msg) {
    let payload = JSON.stringify(msg);

    if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ipc) {
        window.webkit.messageHandlers.ipc.postMessage(payload);
    } else if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
        window.chrome.webview.postMessage(payload);
    } else {
        throw new Error("Unable to find webview postMessage implementation");
    }
}

let INSTANCE = null;

// NOTE: Exactly one instance of this should exist so use ChannelWebView.global() to get it.
export class ChannelWebView {
    static global() {
        if (!INSTANCE) {
            INSTANCE = new ChannelWebView();
        }

        return INSTANCE;
    }

    _base: ChannelMessageBased;

    constructor() {
        if (window.__on_message) {
            throw new Error('Duplicate channels being created');
        }

        this._base = new ChannelMessageBased(post_message_webview);

        window.__on_message = this._base.handle_message;
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