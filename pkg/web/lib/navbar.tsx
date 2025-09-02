import React from "react";
import { Router } from "./router";

export interface NavbarProps {
    title: string,
    links?: NavbarLinkOptions[]
}

export interface NavbarLinkOptions {
    name: string,
    to: string,
    right_divider?: boolean,
}

export class NavbarBase extends React.Component<NavbarProps> {
    render() {
        let links = [];
        (this.props.links || []).map(((link, i) => {
            links.push(
                <li key={i + '-a'} className="nav-item">
                    <NavbarLink to={link.to}>{link.name}</NavbarLink>
                </li>
            );

            if (link.right_divider) {
                links.push(
                    <li key={i + '-b'} className="nav-item">
                        <div className="nav-link" style={{ paddingLeft: 0, paddingRight: 0 }}>
                            |
                        </div>
                    </li>
                );
            }
        }));

        return (
            <nav className="navbar navbar-expand-lg navbar-dark bg-dark">
                <div className="container">
                    <a className="navbar-brand" href="/" onClick={(e) => {
                        e.preventDefault();
                        Router.global().goto('/');
                    }}>
                        {this.props.title}
                    </a>
                    <button className="navbar-toggler" type="button">
                        <span className="navbar-toggler-icon"></span>
                    </button>
                    <div className="collapse navbar-collapse" id="navbarNav">
                        <ul className="navbar-nav">
                            {links}
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
