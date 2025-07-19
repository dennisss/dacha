import React from "react";

import { Title } from "pkg/web/lib/title";
import { Button } from "pkg/web/lib/button";
import { CardError } from "pkg/cnc/monitor/js/card_error";
import { Navbar } from "./navbar";
import { maybe_redirect_to_referer } from "./referer";

export interface LoginPageProps {
    channel: any
}

interface LoginPageState {
    _user: string;
    _pass: string;
    _error: string | null;
}

export class LoginPage extends React.Component<LoginPageProps, LoginPageState> {
    state = {
        _user: '',
        _pass: '',
        _error: null
    }

    _form_ref: React.Ref<HTMLFormElement> = React.createRef();

    constructor(props: LoginPageProps) {
        super(props);
    }

    _on_form_submit = (e) => {
        e.preventDefault();
    }

    _on_click_login = (done) => {
        // TODO: Get browser password saving working for this (also for the redirect path)

        this.props.channel.call('cluster.UserSessionAuthentication', 'Login', {
            user_name: this.state._user,
            user_password: this.state._pass,
        }).then((res) => {
            if (res.status.ok()) {
                if (maybe_redirect_to_referer()) {
                    return;
                }

                location.reload();
            } else {
                this.setState({ _error: 'Login failed: ' + res.status.toString() });
            }

            done();
        });

        // this._form_ref.current.submit()
    }

    render() {
        return (
            <div>
                <Title value="Login" />
                <Navbar />

                <div className="container" style={{ paddingTop: 20, paddingBottom: 20, position: 'relative' }}>
                    <div style={{ maxWidth: 500, margin: '0 auto' }}>
                        {this.state._error != null ? (
                            <div style={{ paddingBottom: 20 }}>
                                <CardError>{this.state._error}</CardError>
                            </div>
                        ) : null}

                        <form action="" onSubmit={this._on_form_submit} ref={this._form_ref} method="POST">
                            <div className="mb-3">
                                <label className="form-label">User Name</label>
                                <input
                                    type="text"
                                    autoComplete="username"
                                    className="form-control"
                                    required
                                    autoFocus
                                    value={this.state._user} onChange={(e) => this.setState({ _user: e.target.value })} />
                            </div>
                            <div className="mb-3">
                                <label className="form-label">Password</label>
                                <input
                                    type="password"
                                    autoComplete="current-password"
                                    className="form-control"
                                    required
                                    autoFocus

                                    value={this.state._pass} onChange={(e) => this.setState({ _pass: e.target.value })} />

                            </div>
                            <div>
                                <Button type="submit" preset="primary" onClick={this._on_click_login} style={{ width: '100%' }}>Login!</Button>
                            </div>
                        </form>
                    </div>
                </div>
            </div>
        );

    }
}