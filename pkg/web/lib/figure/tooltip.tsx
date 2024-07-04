// This is code for the tooltips that show up when a user hovers over a point in a graph.

import React from "react";
import { Point } from "./types";


export interface TooltipProps {
    data: TooltipDataContainer
}

export interface TooltipData {
    // Position of the tooltip relative to the top-left corner of the canvas.
    position: Point;

    right_align: boolean;

    x_value: string;

    lines: TooltipLineEntry[];
}

export interface TooltipLineEntry {
    label: string;
    y_value: string;
    color: string;
}

export class TooltipDataContainer {
    _data: TooltipData | null = null;
    _listeners: (() => void)[] = [];

    update(value: TooltipData | null) {
        this._data = value;
        this._listeners.map((l) => l());
    }

    // TODO: Eventually remove the listeners on data change or component unmount.
    add_listener(f: () => void) {
        this._listeners.push(f);
    }

}

export class Tooltip extends React.Component<TooltipProps> {

    constructor(props: TooltipProps) {
        super(props);
        props.data.add_listener(this._listener);
    }

    shouldComponentUpdate(nextProps: Readonly<TooltipProps>, nextState: Readonly<{}>, nextContext: any): boolean {
        if (nextProps.data != this.props.data) {
            nextProps.data.add_listener(this._listener);
            return true;
        }

        return false;
    }

    _listener = () => {
        this.forceUpdate();
    }

    render() {
        let data = this.props.data._data;
        if (!data) {
            return null;
        }

        // TODO: It would be a better experience if this was a fixed width.

        return (
            <div className="figure-tooltip" style={{ top: data.position.y, left: data.position.x }}>
                <div style={{ fontWeight: 'bold', paddingBottom: 4 }}>
                    {data.x_value}
                </div>
                <div>
                    {data.lines.map((line, i) => {
                        return (
                            <div key={i}>
                                <div style={{ display: 'inline-block', backgroundColor: line.color, width: 10, height: 5 }}></div>
                                <div style={{ display: 'inline-block', minWidth: 60, paddingRight: 4, paddingLeft: 4, fontWeight: 'bold' }}>
                                    {line.label + ':'}
                                </div>
                                <div style={{ textAlign: 'right', display: 'inline-block' }}>
                                    {line.y_value}
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>
        );
    }

}