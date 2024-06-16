import React from "react";

export interface PropertiesTableProps {
    properties: Property[]
    style?: any
    keyWidth?: number
}

export interface Property {
    name: any
    value: any
}

export class PropertiesTable extends React.Component<PropertiesTableProps> {
    render() {
        // break-all is required to prevent the table from consuming >100% of the parent's width.
        return (
            <div style={{ wordBreak: 'break-all' }}>
                <table className="table" style={this.props.style}>
                    <tbody>
                        {this.props.properties.map((prop, i) => {
                            return (
                                <tr key={i}>
                                    <td style={{ whiteSpace: 'nowrap', width: (this.props.keyWidth || 1) }}>
                                        {prop['name']}
                                    </td>
                                    <td>
                                        <div style={{ width: '100%', overflowX: 'hidden' }}>
                                            {prop['value']}
                                        </div>

                                    </td>
                                </tr>
                            );
                        })}
                    </tbody>
                </table>
            </div>
        );
    }
};