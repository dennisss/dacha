import React from "react";
import { NavbarBase, NavbarLinkOptions } from "pkg/web/lib/navbar";
import { DARK_MODE } from "pkg/web/lib/dark";

export const NAVBAR_HEIGHT = 56;

export class Navbar extends React.Component<{ extraLink?: NavbarLinkOptions, togglerClick?: any, togglerActive?: any }> {
    render() {
        let links: NavbarLinkOptions[] = [
            { name: "Cameras", to: "/ui/cameras" },
            { name: "World", to: "/ui/world" },

        ];

        let extra_link = this.props.extraLink;
        if (extra_link) {
            links.splice(0, 0, {
                ...extra_link,
                right_divider: true
            });
        }

        return (
            <NavbarBase dark={DARK_MODE.get()} fullWidth={true} title="Mocap" links={links} togglerClick={this.props.togglerClick} togglerActive={this.props.togglerActive} />
        );
    }
};
