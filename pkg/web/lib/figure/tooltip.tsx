// This is code for the tooltips that show up when a user hovers over a point in a graph.

import React from "react";
import { Point } from "./types";


export interface TooltipProps {
    data: TooltipData
}

export interface TooltipData {
    // Position of the tooltip relative to the top-left corner of the canvas.
    position: Point,

    right_align: boolean,

    x_value: string,

    lines: {
        label: string,
        y_value: string,
        color: string
    }[]
}

export class Tooltip extends React.Component<TooltipProps> {

    render() {
        let data = this.props.data;

        // TODO: It would be a better experience if this was a fixed width.

        return (
            <div style={{ position: 'absolute', top: data.position.y, left: data.position.x, padding: 5, backgroundColor: '#fff', border: '1px solid #ccc', fontSize: 12 }}>
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