import React from "react";
import { Router } from "pkg/web/lib/router";

export interface NavbarLinkOptions {
    name: string,
    to: string
}

export class Navbar extends React.Component<{ extraLink?: NavbarLinkOptions }> {
    render() {
        let extra_link = this.props.extraLink;

        return (
            <nav className="navbar navbar-expand-lg navbar-dark bg-dark">
                <div className="container">
                    <a className="navbar-brand" href="/" onClick={(e) => {
                        e.preventDefault();
                        Router.global().goto('/');
                    }}>
                        CNC Monitor
                    </a>
                    <button className="navbar-toggler" type="button">
                        <span className="navbar-toggler-icon"></span>
                    </button>
                    <div className="collapse navbar-collapse" id="navbarNav">
                        <ul className="navbar-nav">
                            {extra_link ? (
                                <>
                                    <li className="nav-item">
                                        <NavbarLink to={extra_link.to}>{extra_link.name}</NavbarLink>
                                    </li>
                                    <li className="nav-item">
                                        <div className="nav-link" style={{ paddingLeft: 0, paddingRight: 0 }}>
                                            |
                                        </div>
                                    </li>
                                </>
                            ) : null}
                            <li className="nav-item">
                                <NavbarLink to="/ui/machines">Machines</NavbarLink>
                            </li>
                            <li className="nav-item">
                                <NavbarLink to="/ui/files">Files</NavbarLink>
                            </li>
                        </ul>
                    </div>
                </div>
            </nav>
        );
    }
};

interface NavbarLinkProps {
    to: string,
    children: any,
}

class NavbarLink extends React.Component<NavbarLinkProps> {
    _on_click = (e: any) => {
        e.preventDefault();
        Router.global().goto(this.props.to);
    }

    render() {
        let active = this.props.to == Router.global().current_path();
        return (
            <a className={"nav-link" + (active ? " active" : "")} href={this.props.to} onClick={this._on_click}>{this.props.children}</a>
        );
    }
}
