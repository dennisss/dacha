import React from 'react';
import ReactDOM from 'react-dom';

import { Channel } from "pkg/web/lib/rpc";


/*
Bundle using:
./node_modules/.bin/webpack -c ./pkg/sensor_monitor/webpack.config.js --watch



*/



class GraphCard extends React.Component {
    render() {
        return (
            <div className="card">
                <div className="card-header">
                    Temperature
                </div>
                <div className="card-body">
                    <Figure />
                </div>
            </div>
        );
    }

}

class NavBar extends React.Component {
    render() {
        return (
            <nav className="navbar navbar-dark bg-dark">
                <div className="container-fluid">
                    <a className="navbar-brand" href="#">Sensor Monitor</a>
                </div>
            </nav>
        );
    }
}

interface DropDownProps {
    inner_focus?: boolean,
    items: React.ReactElement[]
}

class DropDown extends React.Component<DropDownProps> {

    state = {
        open: false
    }

    _root_el: HTMLDivElement;

    _on_focus(e: React.FocusEvent<HTMLDivElement>) {
        this.setState({ open: true });
    }

    _on_blur(e: React.FocusEvent<HTMLDivElement>) {
        if (this.props.inner_focus) {
            setTimeout(() => {
                let el = document.activeElement;
                while (el !== null) {
                    if (el === this._root_el) {
                        return;
                    }

                    el = el.parentElement;
                }

                this.setState({ open: false });
            });
        } else {
            this.setState({ open: false });
        }
    }

    render() {
        return (
            <div ref={(el) => { this._root_el = el; }} onFocus={(e) => this._on_focus(e)} onBlur={(e) => this._on_blur(e)}
                style={{ position: 'relative', display: 'inline-block' }}>
                <button className="btn btn-sm btn-outline-secondary dropdown-toggle" role="button">
                    {this.props.children}
                </button>

                <ul tabIndex={0} className={"dropdown-menu" + (this.state.open ? ' show' : '')} style={{ right: 0 }}>
                    {this.props.items}
                </ul>
            </div>
        );
    }
}

/*
                    {this.props.items.map((item) => {
                        <li><a className="dropdown-item" href="#">Action</a></li>
                    })}

                    <li><a className="dropdown-item" href="#">Another action</a></li>
                    <li><a className="dropdown-item" href="#">Something else here</a></li>

*/


class App extends React.Component {
    render() {
        return (
            <div>
                <NavBar />
                <div className="container-fluid" style={{ paddingTop: '0.75em' }}>
                    <div style={{ paddingBottom: 10, textAlign: 'right' }}>
                        <DropDown items={[]}></DropDown>
                    </div>

                    <GraphCard />
                </div>
            </div>
        );
    }
};


let node = document.getElementById("app-root");
console.log("Place in", node);
ReactDOM.render(<App />, node)

console.log("Hello world");