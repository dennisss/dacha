import React from "react";

export interface EditInputProps {
    value: string
    onChange: (value: string, done: () => void) => void
}

interface EditInputState {
    _user_value: string | null
}

export class EditInput extends React.Component<EditInputProps, EditInputState> {

    state = {
        _user_value: null
    }

    _on_change = (e) => {
        this.setState({ _user_value: e.target.value });
    }

    _on_key_up = (e) => {
        if (e.key == 'Enter') {
            this._commit();
        }
    }

    _commit = () => {
        if (this.state._user_value === null) {
            return;
        }

        this.props.onChange(this.state._user_value, () => {
            this.setState({ _user_value: null });
        });
    }

    render() {
        let value = this.props.value || '';
        let custom_value = false;
        if (this.state._user_value !== null) {
            value = this.state._user_value;
            custom_value = true;
        }

        return (
            <div className="input-group">
                <input type="text" className="form-control" value={value} onChange={this._on_change} onKeyUp={this._on_key_up} />
                {custom_value ? <>
                    <button className="btn btn-outline-dark" onClick={this._commit} style={{ lineHeight: 1 }}>
                        <span className="material-symbols-outlined">check</span>
                    </button>
                    <button className="btn btn-outline-dark" onClick={() => this.setState({ _user_value: null })} style={{ lineHeight: 1 }}>
                        <span className="material-symbols-outlined">close</span>
                    </button>
                </> : null}

            </div>
        );

    }
}

