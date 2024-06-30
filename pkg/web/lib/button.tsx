import React from "react";
import { SpinnerInline } from "./spinner";
import { shallow_copy } from "./utils";


interface ButtonProps {
    preset?: string
    disabled?: boolean
    style?: any
    active?: boolean

    // function(function(string))
    onClick: any

    spin?: boolean
}

interface ButtonState {
    _waiting: boolean
}

// A button which switches to a spinner when clicked to signify that some async running operation
// is happening.
export class Button extends React.Component<ButtonProps, ButtonState> {
    state = {
        _waiting: false
    };

    _mounted: boolean = true;

    componentWillUnmount(): void {
        this._mounted = false;
    }

    _on_click = (e) => {
        if (this.state._waiting || this.props.spin || this.props.disabled) {
            return;
        }

        this.setState({
            _waiting: true
        });

        this.props.onClick(() => {
            if (this._mounted) {
                this.setState({ _waiting: false });
            }
        })
    }

    render() {
        let preset = this.props.preset || 'primary';

        let style = shallow_copy(this.props.style || {});
        style.position = 'relative';

        let spin = this.state._waiting || this.props.spin;

        return (
            <button disabled={spin || this.props.disabled} type="button" className={"btn btn-" + preset + (this.props.active ? " active" : "")} style={style} onClick={this._on_click}>
                <div>
                    <div style={{ opacity: (spin ? 0 : null) }}>
                        {this.props.children}
                    </div>
                    {spin ? (
                        <div style={{ position: 'absolute', textAlign: 'center', width: '100%', left: 0, top: '50%', transform: 'translate(0, -50%)' }}>
                            <SpinnerInline />
                        </div>
                    ) : null}
                </div>
            </button>
        );
    }
}