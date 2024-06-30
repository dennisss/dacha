import React from "react";
import ReactDOM from "react-dom";
import { VideoPlayer } from "./player";
// import { Channel } from "pkg/web/lib/rpc";


class App extends React.Component<{}, {}> {
    // _channel: Channel;

    constructor(props: {}) {
        super(props);
        // this.state = {
        //     _proto: null
        // };

        // this._channel = new Channel(`${window.location.protocol}//${window.location.hostname}:${global.vars.rpc_port}`);
    }


    render() {

        return (
            <div className="container-fluid">
                <VideoPlayer src="/camera" />
            </div>
        );
    }
};


let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)