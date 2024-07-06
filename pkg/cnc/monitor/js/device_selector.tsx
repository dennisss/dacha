import React from "react";
import { PageContext } from "./page";
import { watch_entities } from "./rpc_utils";
import { compare_values } from "pkg/web/lib/utils";

export interface DevicePickerProps {
    context: PageContext;

    // Sparse device selector used for matching devices.
    // (this is the main value that is being refined by this input).
    // DeviceSelector proto | null
    selector: any;

    // If known, verbose information on the device matched by 'selector'.
    // DeviceSelector proto | null
    device: any;

    // The actual currently connected device that was matched by some current/previous value of 'selector'.
    //
    // This is mainly needs to be provided so that we know that it should be selectable from the list of available devices (even though it is used by a machine).
    connected_device: any;

    // Called when 'selector' / 'device' should be updated based on user input.
    onChange: (selector: any, device: any) => void;

    // Returns whether or not to allow a 
    filter: (device: any) => boolean;
}

interface DevicePickerState {
    _devices: any[] | null
}

function format_fixed_width_hex(v: number, num_bytes: number): string {
    return v.toString(16).padStart(2 * num_bytes, '0');
}

// Picks a device from all available ones.
//
// TODO: This still has some issues:
// - If the selector contains fields not available in the device, they won't be shown.
// - This is a risk that during disconnect from old / connect to new device cycles, the user may not provide the correct 'device' prop to us that matches what was used for selection.
// - Immediately upon disconnecting from a device, it will still show up with 'used_by_machine_id' so we won't be able to re-pick it. The current hacky solution to this is that watch_entities will probably return a new device set very soon so we just need to wait a second.
export class DeviceSelectorInput extends React.Component<DevicePickerProps, DevicePickerState> {

    state = {
        _devices: null,
    }

    _abort_controller: AbortController = new AbortController();

    constructor(props: DevicePickerProps) {
        super(props);

        watch_entities(props.context, { entity_type: 'DEVICE' }, (msg) => {
            let devices = msg.devices || [];
            devices.sort((a, b) => compare_values(a.id, b.id));

            this.setState({ _devices: devices });
        }, { abort_signal: this._abort_controller.signal });

    }

    componentWillUnmount(): void {
        this._abort_controller.abort();
    }

    _on_select_device = (e) => {
        let selected_path = e.target.value;

        if (selected_path == 'none') {
            this.props.onChange(null, null);
            return;
        }

        let selected_device = null;
        let selector = null;
        (this.state._devices || []).map((device) => {
            if (device.info.path == selected_path) {
                selected_device = device.info;
                selector = get_default_selector(device.info);
            }
        });

        if (!selected_device) {
            return;
        }

        this.props.onChange(selector, selected_device);
    }

    _filter_device(device_entry: any): boolean {
        let is_current_connected = this.props.connected_device ?
            this.props.connected_device.path == device_entry.info.path : false;

        if (!is_current_connected && device_entry.used_by_machine_id) {
            return false;
        }

        return this.props.filter(device_entry.info);
    }

    render() {

        let devices = (this.state._devices || []).slice();

        let selector = this.props.selector;

        let selected_path = null;
        if (this.props.device) {
            selected_path = this.props.device.path;
        }
        if (!selector) {
            selected_path = 'none';
        }

        let selected_device = null;
        for (var i = 0; i < devices.length; i++) {
            if (devices[i].info.path == selected_path) {
                selected_device = devices[i];
                continue;
            }

            if (!this._filter_device(devices[i])) {
                devices.splice(i, 1);
                i -= 1;
            }
        }

        // This should only happen if there is some the watch_entities data is stale or newer than the user's data.
        // if (!selected_device && this.props.device) {
        //     selected_device = { info: this.props.device };
        //     devices.push(selected_device);
        // }

        /*
        Special value of 'None' can be used to deselect things.
        */

        return (
            <div>
                <div style={{ fontSize: '0.8em' }}>
                    Selected device:
                </div>

                <select value={selected_path || ''} className="form-select" style={{ fontSize: '0.8em', marginBottom: 10 }}
                    onChange={this._on_select_device}>
                    <option value="none">None</option>

                    {selector && !selected_device ? (
                        <option value={selected_path || ''}>Missing Device</option>
                    ) : null}

                    {devices.map((device) => {

                        let name = 'Unknown Device';

                        if (device.info.usb) {
                            let usb = device.info.usb;
                            name = `[USB ${format_fixed_width_hex(usb.vendor, 2)}:${format_fixed_width_hex(usb.product, 2)}] ${usb.vendor_name || ''} | ${usb.product_name || ''}`;
                        } else if (device.info.fake !== undefined) {
                            name = 'Fake Device #' + device.info.fake;
                        } else {
                            console.error('Unknown device', device);
                        }

                        let path = device.info.path;

                        return (
                            <option key={path} value={path}>
                                {name}
                            </option>
                        )
                    })}
                </select>

                {selected_device != null || selector != null ? (
                    <div>
                        <div style={{ fontSize: '0.8em' }}>
                            Properties to match:
                        </div>
                        <DeviceSelectorEditor device={selected_device ? selected_device.info : null} selector={selector}
                            onChange={(selector) => {
                                this.props.onChange(selector, selected_device ? selected_device.info : null);
                            }} />
                    </div>
                ) : null}
            </div>
        )
    }
}

interface DeviceSelectorEditorProps {
    // The full verbose DeviceSelector
    // If this is null|undefined, then we will 
    device: any

    // Value of the selector.
    // A 'null|undefined' selector implies that no device is being selected. 
    selector: any

    onChange: (value: any) => void
}

interface DeviceSelectorField {
    path: string

    // If true, this field is selected by default if present.
    default?: boolean;

    formatter?: (v: any) => string;
}

// TODO: Format the number fields as hex.
const SUPPORTED_FIELDS: DeviceSelectorField[] = [
    { path: 'usb.product', default: true, formatter: (v) => format_fixed_width_hex(v, 2) },
    { path: 'usb.product_name' },
    { path: 'usb.vendor', default: true, formatter: (v) => format_fixed_width_hex(v, 2) },
    { path: 'usb.vendor_name' },
    { path: 'usb.serial_number', default: true },
    { path: 'fake' }
]

function get_field_value(obj: any, path: string): any {
    let parts = path.split('.');
    for (var i = 0; i < parts.length; i++) {
        if (obj === undefined || obj === null) {
            return undefined;
        }

        obj = obj[parts[i]];
    }

    return obj;
}

function set_field_value(obj: any, path: string, value: any) {
    let parts = path.split('.');
    for (var i = 0; i < parts.length - 1; i++) {
        if (obj[parts[i]] === undefined || obj[parts[i]] === null) {
            obj[parts[i]] = {}
        }

        obj = obj[parts[i]];
    }

    obj[parts[parts.length - 1]] = value;
}

function get_default_selector(device: any): any {
    return selector_for_fields(device, SUPPORTED_FIELDS.filter((field) => field.default));
}

function selector_for_fields(device: any, fields: DeviceSelectorField[]): any {
    let out = {};

    let found_some = false;
    fields.map((field) => {
        let value = get_field_value(device, field.path);
        if (value !== undefined) {
            set_field_value(out, field.path, value)
            found_some = true;
        }
    });

    if (!found_some) {
        return null;
    }

    return out;
}

function fields_in_selector(selector: any): DeviceSelectorField[] {
    return SUPPORTED_FIELDS.filter((field) => {
        return get_field_value(selector, field.path) !== undefined;
    });
}

class DeviceSelectorEditor extends React.Component<DeviceSelectorEditorProps> {

    /*
    componentDidMount() {
        this.componentDidUpdate();
    }

    componentDidUpdate() {

        if (this.props.device) {
            // If the device changes, it is possible that the device may have newer values of some fields that aren't reflected in the selector.
            // TODO: Need to normalize for map field ordering here. 
            let selector = selector_for_fields(this.props.device, fields_in_selector(this.props.selector));
            if (JSON.stringify(selector) != JSON.stringify(this.props.selector)) {
                this.props.onChange(selector);
            }
        }
    }
    */

    render() {

        let device = this.props.device;
        let selector = this.props.selector;

        // TODO: Print a special 'no fields available' message if not of the fields we know of have any values.
        return (
            <div style={{ wordBreak: 'break-all', fontSize: '0.8em' }}>
                <table className="table">
                    <tbody>
                        {SUPPORTED_FIELDS.map((field, i) => {
                            let value = get_field_value(device, field.path);
                            let sel_value = get_field_value(selector, field.path);
                            if (value === undefined && sel_value === undefined) {
                                return;
                            }

                            // TODO: If value != sel_value, show both as separate rows with only the one in the selector actually marked as checked.

                            let picked = sel_value !== undefined;

                            let value_to_show = sel_value || value;
                            if (field.formatter) {
                                value_to_show = (field.formatter)(value_to_show);
                            }

                            return (
                                <tr key={i}>
                                    <td style={{ width: 1 }}>
                                        <input className="form-check-input" type="checkbox" checked={picked} onChange={(e) => {

                                            let checked = e.target.checked;

                                            let fields = fields_in_selector(selector);
                                            let idx = fields.indexOf(field);
                                            if (idx >= 0) {
                                                fields.splice(idx, 1);
                                            }

                                            if (checked) {
                                                fields.push(field);
                                            }

                                            // TODO: Need null/undefined comparison instead of '||'
                                            this.props.onChange(selector_for_fields(device || selector, fields));
                                        }} />
                                    </td>
                                    <td style={{ whiteSpace: 'nowrap', width: 1 }}>
                                        {field.path}
                                    </td>
                                    <td>
                                        <div style={{ width: '100%', overflowX: 'hidden' }}>
                                            {/* TODO: Need null/undefined comparison instead of '||' */}
                                            {value_to_show}
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



}
