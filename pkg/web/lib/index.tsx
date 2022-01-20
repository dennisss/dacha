import React from "react";
import ReactDOM from "react-dom";
import { Channel } from "pkg/web/lib/rpc";

class App extends React.Component<{}, { _result?: number }> {
    constructor(props: {}) {
        super(props);
        this.state = {
            _result: null
        };
    }

    _on_submit = () => {
        let channel = new Channel("http://localhost:8001");
        channel.call("Adder", "Add", { "x": 4, "y": 5 })
            .then((res) => {
                let z = res.responses[0].z;
                this.setState({
                    _result: z
                });
            });
    }

    render() {
        return (
            <div style={{ padding: 20 }}>
                <input type="text" className="form-control" style={{ display: "inline-block", width: 100 }} />
                <span style={{ fontSize: 22, padding: "0 10px" }}>+</span>
                <input type="text" className="form-control" style={{ display: "inline-block", width: 100 }} />
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