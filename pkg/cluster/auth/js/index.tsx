import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";
import { LoginPage } from "./login";
import { ProfilePage } from "./profile";
import { maybe_redirect_to_referer } from "./referer";

interface AppState {
    session_info: any | null;
}

class App extends React.Component<{}, AppState> {

    state: AppState = {
        session_info: null
    };

    _channel: Channel = new Channel('/rpc');

    constructor(props: {}) {
        super(props);

        this._channel.call('cluster.UserSessionAuthentication', 'SessionInfo', {}).then((res) => {
            let session_info = res.responses[0];

            if (session_info.user) {
                if (maybe_redirect_to_referer()) {
                    return;
                }
            }

            this.setState({ session_info: session_info });
        });
    }

    render() {
        let session_info = this.state.session_info;
        if (!session_info) {
            return <div>Loading...</div>;
        }

        return (
            <div className="app-outer">
                {session_info.user ? (
                    <ProfilePage channel={this._channel} session_info={session_info} />
                ) : (
                    <LoginPage channel={this._channel} />
                )}
            </div>
        );
    }
};

let node = document.getElementById("app-root");
ReactDOM.render(<App />, node)