import React from "react";

export class Navbar extends React.Component {
    render() {
        return (
            <nav className="navbar navbar-dark bg-dark">
                <div className="container-fluid">
                    <a className="navbar-brand" href="#">Cluster Authentication</a>
                </div>
            </nav>
        );
    }
}
