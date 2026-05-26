import React from "react";

import { render_property_list } from "pkg/media/camera/js/property";
import { deep_copy, shallow_copy } from "pkg/web/lib/utils";


export class MocapCameraControls extends React.Component<{ config: any, camera_controls: any, onChange: any }> {

    _control_changed = (prop, new_value) => {
        let config = shallow_copy(this.props.config);

        config[prop.id] = new_value.int32_value;

        if (prop.id == 'strobe_power') {
            config[prop.id] /= 100.0;
        }

        this.props.onChange(config);
    }

    _on_camera_control_changed = (prop, new_value) => {
        let config = deep_copy(this.props.config);

        config.camera_controls.states.map((c) => {
            if (c.id == prop.id) {
                c.current_value = new_value;
            }
        });

        this.props.onChange(config);
    }


    render() {

        let config = this.props.config;

        let props = [
            {
                id: "frame_rate",
                spec: {
                    name: "Frame Rate",
                    type: "INT32",
                    // TODO: Limit to 119 (currently awkward to do this since it will snap to 115 due to t he step size)
                    min_value: { int32_value: 0 },
                    max_value: { int32_value: 120 },
                    step: { int32_value: 5 },
                    default_value: { int32_value: 0 }
                },
            },
            {
                id: "exposure_micros",
                spec: {
                    name: "Exposure (us)",
                    type: "INT32",
                    min_value: { int32_value: 20 },
                    max_value: { int32_value: 6000 },
                    step: { int32_value: 1 },
                    default_value: { int32_value: 20 }
                },

            },
            {
                id: "pixel_threshold",
                spec: {
                    name: "Threshold",
                    type: "INT32",
                    min_value: { int32_value: 1 },
                    max_value: { int32_value: 255 },
                    step: { int32_value: 1 },
                    default_value: { int32_value: 80 }
                },

            },
            {
                id: "strobe_power",
                spec: {
                    name: "Strobe %",
                    type: "INT32",
                    min_value: { int32_value: 0 },
                    max_value: { int32_value: 100 },
                    step: { int32_value: 5 },
                    default_value: { int32_value: 10 }
                },
            },
        ];

        let prop_states = {
            "frame_rate": { current_value: { int32_value: config.frame_rate } },
            "exposure_micros": { current_value: { int32_value: config.exposure_micros } },
            "pixel_threshold": { current_value: { int32_value: config.pixel_threshold } },
            "strobe_power": { current_value: { int32_value: Math.round(config.strobe_power * 100) } }
        };


        let camera_states = {};

        if (config.camera_controls) {
            config.camera_controls.states.map((s) => {
                camera_states[s.id] = s;
            });
        }

        return (
            <>
                {render_property_list(props, prop_states, this._control_changed)}
                {render_property_list(this.props.camera_controls.children || [], camera_states, this._on_camera_control_changed)}
            </>
        );

    }
}
