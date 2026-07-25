import React from "react";

import { render_property_list } from "pkg/media/camera/js/property";
import { deep_copy, shallow_copy } from "pkg/web/lib/utils";


export class MocapCameraControls extends React.Component<{ config: any, camera_controls: any, onChange: any }> {

    _control_changed = (prop, new_value) => {
        let config = shallow_copy(this.props.config);

        let parts = prop.id.split('.');
        let c = config;
        for (var i = 0; i < parts.length; i++) {
            if (i == parts.length - 1) {
                if (prop.id == "mjpeg.thresholded" || prop.id == 'leds_on') {
                    c[parts[i]] = new_value.int32_value ? true : false
                } else {
                    c[parts[i]] = new_value.int32_value;
                }


            } else {
                c = c[parts[i]];
            }
        }

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
            {
                id: "mjpeg.quality",
                spec: {
                    name: "MJPEG Quality",
                    type: "INT32",
                    min_value: { int32_value: 0 },
                    max_value: { int32_value: 100 },
                    step: { int32_value: 5 },
                    default_value: { int32_value: 100 }
                }
            },
            {
                id: "mjpeg.max_fps",
                spec: {
                    name: "MJPEG FPS",
                    type: "INT32",
                    min_value: { int32_value: 1 },
                    max_value: { int32_value: 30 },
                    step: { int32_value: 1 },
                    default_value: { int32_value: 5 }
                }
            },

            {
                id: "mjpeg.downsampling",
                spec: {
                    name: "MJPEG Downsampling",
                    type: "ENUM",
                    values: [
                        {
                            value_name: "1x (full res)",
                            int32_value: 1
                        },
                        {
                            value_name: "2x (quarter res)",
                            int32_value: 2
                        },
                    ],
                }
            },
            {
                id: "mjpeg.thresholded",
                spec: {
                    name: "MJPEG Thresholded",
                    type: "BOOL"
                }
            },
            {
                id: "leds_on",
                spec: {
                    name: "LEDs On",
                    type: 'BOOL'
                }
            }
        ];

        let prop_states = {
            "frame_rate": { current_value: { int32_value: config.frame_rate } },
            "exposure_micros": { current_value: { int32_value: config.exposure_micros } },
            "pixel_threshold": { current_value: { int32_value: config.pixel_threshold } },
            "strobe_power": { current_value: { int32_value: Math.round(config.strobe_power * 100) } },
            "mjpeg.quality": { current_value: { int32_value: config.mjpeg.quality } },
            "mjpeg.max_fps": { current_value: { int32_value: config.mjpeg.max_fps } },
            "mjpeg.downsampling": { current_value: { int32_value: config.mjpeg.downsampling } },
            "mjpeg.thresholded": { current_value: { int32_value: config.mjpeg.thresholded ? 1 : 0 } },
            "leds_on": { current_value: { int32_value: config.leds_on ? 1 : 0 } }
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
