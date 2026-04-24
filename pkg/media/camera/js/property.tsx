import React from "react";

export function render_group_property(prop, on_change) {
    if (!prop.spec || prop.spec.type != 'GROUP') {
        return <div key={prop.id}>Unknown: {prop.id}</div>;
    }

    if ((prop.children || []).length == 0) {
        return null;
    }

    return (
        <div key={prop.id} className="card" style={{ marginBottom: 10 }}>
            <div className="card-header">
                {prop.spec.name || prop.id}
            </div>
            <div className="card-body" style={{ padding: 10 }}>
                {render_property_list(prop.children, on_change)}
            </div>
        </div>
    );
}


export function render_property_list(props, on_change) {
    return (
        <div>
            <div style={{ wordBreak: 'break-all' }}>
                <table className="table">
                    <tbody>
                        {(props || []).map((prop) => {
                            if (prop.spec && prop.spec.type == 'GROUP') {
                                return null;
                            }

                            return (
                                <tr key={prop.id}>
                                    <td style={{ whiteSpace: 'nowrap', width: 1, verticalAlign: 'middle' }}>
                                        {prop.spec.name || prop.id}
                                    </td>
                                    <td style={{ verticalAlign: 'middle' }}>
                                        <div style={{ width: '100%', overflowX: 'hidden' }}>
                                            {render_property_value(prop, on_change)}
                                        </div>
                                    </td>
                                </tr>
                            )
                        })}
                    </tbody>
                </table>
            </div>
            {(props || []).map((prop) => {
                // Rendering nested groups outside of the table.

                if (prop.spec && prop.spec.type != 'GROUP') {
                    return null;
                }

                return render_group_property(prop, on_change);
            })}
        </div>


    );
}

export function render_property_value(prop, on_change) {
    prop.spec = prop.spec || {};

    if (prop.spec.values || prop.spec.type == 'ENUM') {

        /*
        TODO: Currently enums can either be strings or int32s.

        int32 int32_value = 2;
        string string_value = 4;
        */

        return (
            <select className="form-control" style={{ fontSize: '0.8em' }} value="">
                {(prop.spec.values || []).map((value, i) => {
                    return (
                        <option key={i}>{value.value_name}</option>
                    );
                })}
            </select>

        );
    }

    if (prop.spec.type == 'BOOL') {
        return (
            <input type="checkbox" checked={false} />
        );
    }

    if (prop.spec.type == 'INT32') {
        return (
            <div style={{ display: "flex" }}>
                <div style={{ flexGrow: 1 }}>
                    <input style={{ width: '100%', verticalAlign: 'middle', padding: '10px 0' }} type="range"
                        min={prop.spec.min_value.int32_value || 0}
                        max={prop.spec.max_value.int32_value || 0}
                        step={prop.spec.step.int32_value || 0}
                        value={prop.current_value.int32_value || 0}

                        onChange={(e) => {
                            let v = { int32_value: e.target.valueAsNumber };
                            on_change(prop, v);
                        }}
                    />
                </div>
                <div style={{ width: 100, marginLeft: 15 }}>
                    <input className="form-control" type="number"
                        value={prop.current_value.int32_value || 0}
                        onChange={(e) => {
                            let v = { int32_value: e.target.valueAsNumber };
                            on_change(prop, v);
                        }}
                    />
                </div>
            </div>
        );
    }


    return (
        <div>Unknown {prop.spec.type}</div>
    )
}