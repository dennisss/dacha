import React from "react";

import { Title } from "pkg/web/lib/title";
import { Button } from "pkg/web/lib/button";
import { CardError } from "pkg/cnc/monitor/js/card_error";
import { Navbar } from "./navbar";
import { Card, CardBody } from "pkg/cnc/monitor/js/card";

export interface ProfilePageProps {
    channel: any,
    session_info: any,
}

interface ProfilePageState {
    _error: string | null;
    _current_pass: string;
    _new_pass: string;
    _new_pass_confirm: string;
}

export class ProfilePage extends React.Component<ProfilePageProps, ProfilePageState> {
    state: ProfilePageState = {
        _error: null,
        _current_pass: '',
        _new_pass: '',
        _new_pass_confirm: ''
    }

    _form_ref: React.Ref<HTMLFormElement> = React.createRef();

    constructor(props: ProfilePageProps) {
        super(props);
    }

    _on_form_submit = (e) => {
        e.preventDefault();
    }

    _on_click_change_pass = (done) => {
        if (this.state._new_pass != this.state._new_pass_confirm) {
            this.setState({ _error: 'New passwords don\'t match.' });
            done();
            return;
        }

        this.props.channel.call('cluster.UserSessionAuthentication', 'ChangePassword', {
            user_name: this.props.session_info.user.name,
            current_password: this.state._current_pass,
            new_password: this.state._new_pass
        }).then((res) => {
            if (res.status.ok()) {
                this.setState({
                    _error: null,
                    _current_pass: '',
                    _new_pass: '',
                    _new_pass_confirm: ''
                });
            } else {
                this.setState({ _error: 'Change password failed: ' + res.status.toString() });
            }

            done();
        });

    }

    _on_click_logout = (done) => {
        this.props.channel.call('cluster.UserSessionAuthentication', 'Logout', {}).then((res) => {
            if (res.status.ok()) {
                location.reload();
            } else {
                this.setState({ _error: 'Logout failed: ' + res.status.toString() });
            }

            done();
        });
    }

    render() {
        let session_info = this.props.session_info;

        // TODO: Figure out how to get Chrome to save changed passwords.

        return (
            <div>
                <Title value="Profile" />
                <Navbar />

                <div className="container" style={{ paddingTop: 20, paddingBottom: 20, position: 'relative' }}>
                    <div style={{ maxWidth: 500, margin: '0 auto' }}>
                        {this.state._error != null ? (
                            <div style={{ paddingBottom: 20 }}>
                                <CardError>{this.state._error}</CardError>
                            </div>
                        ) : null}

                        <div style={{ paddingBottom: 20 }}>
                            <h3>Logged in as <b>{session_info.user.name}</b></h3>
                        </div>

                        <Card header="Change Password" style={{ marginBottom: 30 }}>
                            <CardBody>
                                <form action="" onSubmit={this._on_form_submit} ref={this._form_ref} method="POST">
                                    <div className="mb-3">
                                        <label className="form-label">Current Password</label>
                                        <input
                                            type="password"
                                            autoComplete="current-password"
                                            className="form-control"
                                            required
                                            autoFocus
                                            value={this.state._current_pass} onChange={(e) => this.setState({ _current_pass: e.target.value, _error: null })} />
                                    </div>
                                    <div className="mb-3">
                                        <label className="form-label">New Password</label>
                                        <input
                                            type="password"
                                            autoComplete="new-password"
                                            className="form-control"
                                            required
                                            autoFocus
                                            value={this.state._new_pass} onChange={(e) => this.setState({ _new_pass: e.target.value, _error: null })} />
                                    </div>
                                    <div className="mb-3">
                                        <label className="form-label">Confirm New Password</label>
                                        <input
                                            type="password"
                                            autoComplete="new-password"
                                            className="form-control"
                                            required
                                            autoFocus
                                            value={this.state._new_pass_confirm} onChange={(e) => this.setState({ _new_pass_confirm: e.target.value, _error: null })} />
                                    </div>
                                    <div>
                                        <Button type="submit" preset="primary" onClick={this._on_click_change_pass} style={{ width: '100%' }}>Change Password!</Button>
                                    </div>
                                </form>
                            </CardBody>
                        </Card>

                        <div>
                            <Button preset="secondary" onClick={this._on_click_logout} style={{ width: '100%' }}>Logout!</Button>
                        </div>
                    </div>
                </div>
            </div>
        );

    }
}