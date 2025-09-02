import React from "react";
import { NavbarBase, NavbarLinkOptions } from "pkg/web/lib/navbar";

export class Navbar extends React.Component {
    render() {
        let links: NavbarLinkOptions[] = [
            { name: "Packs", to: "/ui/packs" },
            { name: "Parts", to: "/ui/parts" },
        ];

        return (
            <NavbarBase title="Inventory" links={links} />
        );
    }
};
