import React from "react";
import { shallow_copy } from "../utils";


export interface FigureLegendEntry {
    id: any;
    name: string;
    color: string;
    visible: boolean;
    focused: boolean;
}

export class FigureLegend extends React.Component<{ entries: FigureLegendEntry[], onChange: (entry: FigureLegendEntry) => void }> {
    render() {
        return (
            <div style={{ paddingTop: 10, fontSize: '0.8em', textAlign: 'center' }}>

                {this.props.entries.map((entry) => {
                    return (
                        <SeriesButton key={entry.id} entry={entry} onChange={this.props.onChange} />
                    );
                })}
            </div>
        );
    }
}

class SeriesButton extends React.Component<{ entry: FigureLegendEntry, onChange: (entry: FigureLegendEntry) => void }> {

    _mouse_enter = () => {
        // The timeout is to ensure that the mouse_exit from another button gets applied before this one is applied.
        // TODO: The better way to do this is to verify at most one button has width >1.
        setTimeout(() => {
            if (this.props.entry.visible) {
                let v = shallow_copy(this.props.entry);
                v.focused = true;
                this.props.onChange(v);
            }
        }, 2);
    }

    _mouse_exit = () => {
        if (this.props.entry.focused) {
            let v = shallow_copy(this.props.entry);
            v.focused = false;
            this.props.onChange(v);
        }
    }

    _on_click = () => {
        let entry = shallow_copy(this.props.entry);

        if (entry.visible) {
            entry.visible = false;
            entry.focused = false;
        } else {
            entry.visible = true;
            // TODO: Ensure that the mouse is still over?
            entry.focused = true;
        }

        this.props.onChange(entry);
    }

    render() {
        let entry = this.props.entry;
        let on = entry.visible;

        return (
            <div className="figure-series-button" onClick={this._on_click} onMouseEnter={this._mouse_enter} onMouseLeave={this._mouse_exit}>
                <div style={{ border: ('1px solid ' + entry.color), display: 'inline-block', marginRight: '1ex', width: 20, height: 10, backgroundColor: (on ? entry.color : null) }}></div>

                {entry.name}
            </div>
        );

    }

}
