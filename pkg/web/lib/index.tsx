import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";

class App extends React.Component<{}, { _a: number, _b: number, _result?: number }> {
    constructor(props: {}) {
        super(props);
        this.state = {
            _a: 0,
            _b: 0,
            _result: null
        };
    }

    _on_submit = () => {
        // TODO: Pull the port number from the embedded data.
        let channel = new Channel("http://localhost:8001");
        channel.call("Adder", "Add", { "x": this.state._a, "y": this.state._b })
            .then((res) => {
                let z = res.responses[0].z || 0;
                this.setState({
                    _result: z
                });
            });
    }

    render() {
        return (
            <div style={{ padding: 20 }}>
                <input type="number" value={this.state._a} onChange={(e) => this.setState({ _a: parseInt(e.target.value) })} className="form-control" style={{ display: "inline-block", width: 100 }} />
                <span style={{ fontSize: 22, padding: "0 10px" }}>+</span>
                <input type="number" value={this.state._b} onChange={(e) => this.setState({ _b: parseInt(e.target.value) })} className="form-control" style={{ display: "inline-block", width: 100 }} />
                <span style={{ fontSize: 22, padding: "0 10px" }}>=</span>
                {this.state._result !== null ? (
                    <span style={{ fontSize: 22, padding: "0 10px" }}>{this.state._result}</span>
                ) : null}
                <br />
                <button className="btn btn-primary" style={{ marginTop: 10 }} onClick={this._on_submit}>Submit</button>
            </div>
        );
    }
};


let node = document.getElementById("app-root");
console.log("Place in", node);
ReactDOM.render(<App />, node)