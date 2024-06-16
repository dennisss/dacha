import React from "react";

export class LabeledCheckbox extends React.Component<{ checked: boolean, onChange: any }> {
    render() {
        return (
            <div className="form-check">
                <input className="form-check-input" type="checkbox" onChange={(e) => this.props.onChange(e.target.checked)} checked={this.props.checked} />
                <label className="form-check-label">
                    {this.props.children}
                </label>
            </div>
        )
    }
}
