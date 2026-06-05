import React from "react";
import { NavbarBase, NavbarLinkOptions } from "pkg/web/lib/navbar";

export class Navbar extends React.Component<{ extraLink?: NavbarLinkOptions }> {
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
            <NavbarBase title="Mocap" links={links} />
        );
    }
};
