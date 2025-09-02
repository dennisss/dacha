import React from "react";
import { Figure } from "pkg/web/lib/figure";
import { EntityKind, FigureOptions, Point, Range } from "pkg/web/lib/figure/types";
import { PageContext } from "pkg/web/lib/page";
import { run_machine_command } from "../rpc_utils";
import { Card, CardBody } from "../card";
import { FigureLegend, FigureLegendEntry } from "pkg/web/lib/figure/legend";
import { MachineUiState } from "./state";
import { clean_point } from "pkg/web/lib/figure/utils";
import { resolve_leveling_state } from "./carvera";
import { Button } from "pkg/web/lib/button";
import { binary_image_to_image } from "./binary_image";
import { shallow_copy } from "pkg/web/lib/utils";


function clean_range(v: any): Range {
    return {
        min: v.min || 0,
        max: v.max || 0
    };
}

function rect_points(min: Point, max: Point): Point[] {
    let x1 = min.x || 0;
    let y1 = min.y || 0;
    let x2 = max.x || 0;
    let y2 = max.y || 0;

    return [
        { x: x1, y: y1 },
        { x: x1, y: y2 },
        { x: x2, y: y2 },
        { x: x2, y: y1 },
    ];
}

function min_width_div<T>(list: T[], i: number, f: (el: T, i: number) => string) {

    let max_chars = 0;
    let value = '';

    list.map((el, j) => {
        let str = f(el, j);
        if (str.length > max_chars) {
            max_chars = str.length;
        }

        if (j == i) {
            value = str;
        }
    });

    return (
        <div style={{ minWidth: max_chars + 'ex' }}>
            {value}
        </div>
    );
}

export interface PositionBoxProps {
    machine: any;
    context: PageContext;
    ui_state: MachineUiState;
}

export class PositionBox extends React.Component<PositionBoxProps> {

    state = {
        _ready: false,
        _toggled_entities: {},
        // 
        _layer_index: null,
    }

    _background_image: HTMLImageElement | null = null;
    _image_cache = new Map();

    constructor(props: PositionBoxProps) {
        super(props);
        this._load()
    }

    async _load() {
        let machine = this.props.machine;
        let work_area = machine.config.work_area;

        if (work_area.background) {

            let img = new Image();
            img.src = '/assets/' + work_area.background.path;

            await new Promise((res, _) => {
                console.log(img.complete);
                if (img.complete) {
                    res(null)
                } else {
                    img.onload = () => {
                        res(null);
                    }
                    img.onerror = (e) => {
                        console.error(e);
                    }
                }
            });

            this._background_image = img;
        }

        setTimeout(() => {
            this.setState({ _ready: true });
        });
    }


    // _state = 

    // TODO: Also have a Z graph which we can click on.


    _get_figure_options(): [FigureOptions, FigureLegendEntry[]] {
        let machine = this.props.machine;

        /*
        We have some state that 
        */

        let legend = [];

        // NOTE: Most of these are empty defaults to be overriden by the specific background generation functions.
        let options = {
            width: '100%',
            max_height: 700,
            aspect_ratio: 1,

            margin: {
                left: 0,
                bottom: 0,
                top: 0,
                right: 0
            },
            font: {
                style: '14px "Noto Sans"',
                size: 14
            },

            x_axis: {
                range: { min: 0, max: 0 },
                ticks: []
            },

            y_axis: {
                range: { min: 0, max: 0 },
                ticks: []
            },

            entities: []
        };


        if (machine.config.work_area.background) {
            this._add_image_background(options);
        } else {
            this._add_grid_background(options);
        }

        this._add_layer_image(options, legend);

        this._add_object_regions(options, legend);

        if (this._add_carvera_preview(options, legend)) {
            //
        } else {
            this._add_work_coordinate_origin(options, legend);
            this._add_program_outline(options, legend);
        }



        let x_pos = null;
        let y_pos = null;
        (machine.state.axis_values || []).map((axis) => {
            if (axis.id == "X") {
                x_pos = (axis.value || [null])[0];
            }
            if (axis.id == "Y") {
                y_pos = (axis.value || [null])[0];
            }
        })

        if (x_pos !== null && y_pos !== null) {
            options.entities.push({
                kind: EntityKind.Circle,
                center: { x: x_pos, y: y_pos },
                color: 'red',
                radius: 5
            });
        }

        return [options, legend];
    }

    _add_work_coordinate_origin(options: FigureOptions, legend: FigureLegendEntry[]) {
        let work_origin = this._get_work_offset() || { x: 0, y: 0 };
        if (!work_origin) {
            return;
        }

        this._add_custom_work_coordinate_origin(work_origin, {
            id: 'work_origin',
            color: '#42f566',
            name: 'Work Origin',
            visible: true,
            focused: false
        }, options, legend);
    }

    _add_custom_work_coordinate_origin(
        work_origin: Point, default_entry: FigureLegendEntry,
        options: FigureOptions, legend: FigureLegendEntry[]
    ) {
        let legend_entry = this.props.ui_state.position_legend().get_or_insert(default_entry);
        legend.push(legend_entry);

        if (!legend_entry.visible) {
            return;
        }

        options.entities.push({
            kind: EntityKind.Path,
            color: legend_entry.color,
            width: legend_entry.focused ? 6 : 3,

            points: [
                { x: work_origin.x + 20, y: work_origin.y },
                { x: work_origin.x, y: work_origin.y }, ,
                { x: work_origin.x, y: work_origin.y + 20 }
            ],
            closed: false
        });

    }

    _get_work_offset(coordinate_system: string | null = null): Point | undefined {
        let machine = this.props.machine;

        let work_coordinates = null;
        if (machine.state.coordinate_systems) {
            machine.state.coordinate_systems.map((c) => {
                if (coordinate_system !== null) {
                    if (coordinate_system != c.gcode) {
                        return;
                    }
                } else if (!c.current) {
                    return;
                }

                work_coordinates = c;
            });
        }

        if (!work_coordinates) {
            return;
        }

        let work_x_origin = null;
        let work_y_origin = null;

        (work_coordinates.offset || []).map((offset) => {
            if (offset.id == 'X') {
                work_x_origin = offset.value[0];
            }
            if (offset.id == 'Y') {
                work_y_origin = offset.value[0];
            }
        });

        if (work_x_origin === null || work_y_origin === null) {
            return;
        }

        return { x: work_x_origin, y: work_y_origin };
    }

    // TODO: Fix this since now we do coordinate specific outlines.
    _add_program_outline(options: FigureOptions, legend: FigureLegendEntry[]) {
        let machine = this.props.machine;
        if (machine.state.connection_state != 'CONNECTED') {
            // Must be connected to have valid workspace offset information.
            return;
        }

        // TODO: Support any coordinate systems more generically by checking for the offsets and drawing all of the outlines individually.
        (machine.state.loaded_program?.file?.program?.bounds || []).map((bounds) => {
            let min_pos = clean_point(bounds.min_position);
            let max_pos = clean_point(bounds.max_position);

            let work_offset = this._get_work_offset(bounds.coordinate_system || '') || { x: 0, y: 0 };
            min_pos.x += work_offset.x;
            min_pos.y += work_offset.y;
            max_pos.x += work_offset.x;
            max_pos.y += work_offset.y;

            this._add_custom_program_outline(min_pos, max_pos, {
                id: 'program_outline',
                color: '#db03fc',
                name: 'Program Outline',
                visible: true,
                focused: false
            }, options, legend);
        });
    }

    _add_custom_program_outline(
        min_pos: Point, max_pos: Point, default_entry: FigureLegendEntry,
        options: FigureOptions, legend: FigureLegendEntry[]
    ) {
        let legend_entry = this.props.ui_state.position_legend().get_or_insert(default_entry);
        legend.push(legend_entry);

        if (!legend_entry.visible) {
            return;
        }

        options.entities.push({
            kind: EntityKind.Path,
            color: legend_entry.color,
            width: legend_entry.focused ? 2 : 1,
            points: rect_points(min_pos, max_pos),
            closed: true
        });
    }

    _add_image_background(options: FigureOptions) {
        let machine = this.props.machine;
        let work_area = machine.config.work_area;
        let bg = work_area.background;

        options.x_axis.range = { min: bg.left || 0, max: (bg.left || 0) + (bg.width || 0) };
        options.y_axis.range = { min: bg.bottom || 0, max: (bg.bottom || 0) + (bg.height || 0) };

        options.entities.push({
            kind: EntityKind.Image,
            image: this._background_image,
            rect: {
                x: bg.left || 0,
                y: bg.bottom || 0,
                width: bg.width || 0,
                height: bg.height || 0
            }
        });
    }

    _add_grid_background(options: FigureOptions) {

        let machine = this.props.machine;

        let x_range = null;
        let y_range = null;
        (machine.config.axes || []).map((axis) => {
            if (axis.id == 'X') {
                x_range = clean_range(axis.range);
            }
            if (axis.id == 'Y') {
                y_range = clean_range(axis.range);
            }
        });

        options.margin = {
            left: 10,
            bottom: 10,
            top: 10,
            right: 10
        };

        if (x_range !== null) {
            options.x_axis = {
                range: x_range,
                ticks: []
            };
        }

        if (y_range !== null) {
            options.y_axis = {
                range: y_range,
                ticks: []
            };
        }

        let work_x = clean_range(machine.config.work_area.x_range);
        let work_y = clean_range(machine.config.work_area.y_range);

        // NOTE: Here we sort of assume that the work_area min is at (0,0)
        // TODO: Have some indicator of whether or not the work_x|y.max ends exactly at a 10mm interval.
        {

            let x = 0;
            while (x < work_x.max) {
                options.entities.push({
                    kind: EntityKind.Line,
                    color: '#aaa',
                    width: (x % 50 == 0 ? 2 : 1),
                    start: { x: x, y: work_y.min },
                    end: { x: x, y: work_y.max }
                });

                x += 10;
            }

            let y = 0;
            while (y < work_y.max) {
                options.entities.push({
                    kind: EntityKind.Line,
                    color: '#aaa',
                    width: (y % 50 == 0 ? 2 : 1),
                    start: { x: work_x.min, y: y },
                    end: { x: work_x.max, y: y }
                });

                y += 10;
            }
        }


        // Border around the whole work area.
        options.entities.push({
            kind: EntityKind.Path,
            color: '#444',
            width: 3,
            points: rect_points({ x: work_x.min, y: work_y.min }, { x: work_x.max, y: work_y.max }),
            closed: true
        });
    }

    _add_layer_image(options: FigureOptions, legend: FigureLegendEntry[]) {
        let ui_state = this.props.ui_state;
        let machine = this.props.machine;
        if (!machine.state.loaded_program) {
            return;
        }

        let preview = machine.state.loaded_program.preview;
        if (!preview || !preview.state.ready) {
            return;
        }

        let layers_data = this._get_layer_data();
        if (!layers_data) {
            return;
        }

        let preview_data = ui_state.program_preview();
        if (!preview_data || !preview_data.data || preview_data.config_key != preview.config_hash || preview_data.file_id != preview.file_id) {
            return;
        }

        if (preview_data.data.layer_images.length == 0) {
            return;
        }

        let legend_entry = this.props.ui_state.position_legend().get_or_insert({
            id: 'layers',
            name: 'Tool Preview',
            color: '#646464',
            visible: true,
            focused: false
        });
        legend.push(legend_entry);

        if (!legend_entry.visible) {
            return;
        }

        // TODO: Clear any old entries from the cache.

        let layer_group = layers_data.layer_groups[layers_data.current_index];

        for (var layer_i = layer_group.start_index; layer_i < layer_group.end_index; layer_i++) {
            let layer = preview.layers[layer_i];
            let layer_image = preview_data.data.layer_images[layer_i];

            let work_offset = this._get_work_offset(layer.coordinate_system || '') || { x: 0, y: 0 };

            let cache_key = preview_data.file_id + ':' + preview_data.config_key + ':' + preview_data.revision + ':' + layer_i;

            let image = this._image_cache.get(cache_key);
            if (!image) {
                image = binary_image_to_image(layer_image, [100, 100, 100, 255]);
                this._image_cache.set(cache_key, image);
            }

            options.entities.push({
                kind: EntityKind.Image,
                image: image,
                rect: {
                    x: work_offset.x + (layer.image.left || 0),
                    y: work_offset.y + (layer.image.bottom || 0),
                    width: layer.image.width || 0,
                    height: layer.image.height || 0
                }
            })
        }


    }

    _add_object_regions(options: FigureOptions, legend: FigureLegendEntry[]) {
        let ui_state = this.props.ui_state;
        let machine = this.props.machine;
        if (!machine.state.loaded_program || !machine.state.loaded_program.file.program) {
            return;
        }

        let objs = machine.state.loaded_program.file.program.objects || [];

        objs.map((obj) => {
            let legend_entry = this.props.ui_state.position_legend().get_or_insert({
                id: 'object_' + (obj.index || 0),
                name: 'Object: ' + obj.name,
                color: '#00bbff',
                visible: true,
                focused: false
            });
            legend.push(legend_entry);

            let cancelled = false;
            let objects_state = machine.state.running_program?.objects;
            if (objects_state) {
                if (objects_state.objects[(obj.index || 0)].cancelled) {
                    cancelled = true;
                }
            }

            let target_color = (cancelled ? '#ff0000' : '#00bbff');
            if (target_color != legend_entry.color) {
                legend_entry = shallow_copy(legend_entry);
                legend_entry.color = target_color;
                this.props.ui_state.position_legend().set(legend_entry);
            }

            // Use min_position and max_position if there is no polygon.
            if (legend_entry.visible) {
                let fill_color = null;
                if (cancelled) {
                    if (legend_entry.focused) {
                        fill_color = 'rgba(255, 0, 0, 0.5)';
                    } else {
                        fill_color = 'rgba(255, 0, 0, 0.1)';
                    }
                } else if (legend_entry.focused) {
                    fill_color = 'rgba(0, 187, 255, 0.5)';
                }

                options.entities.push({
                    kind: EntityKind.Path,
                    color: legend_entry.color,
                    width: 2,
                    points: (obj.polygon || []),
                    fill_color: fill_color,
                    closed: true
                });
            }
        });
    }

    _add_carvera_preview(options: FigureOptions, legend: FigureLegendEntry[]): boolean {
        let machine = this.props.machine;
        let state = this.props.ui_state.carvera_state();
        if (!state || !state.preview) {
            return false;
        }

        let request = resolve_leveling_state(machine, state);
        if (!request) {
            return false;
        }

        // Render work origin
        this._add_custom_work_coordinate_origin(request.origin, {
            id: 'pending_work_origin',
            color: '#42f566',
            name: '(Pending) Work Origin',
            visible: true,
            focused: false
        }, options, legend);


        // Render program outline
        this._add_custom_program_outline(request.program_min, request.program_max, {
            id: 'pending_program_outline',
            color: '#db03fc',
            name: '(Pending) Program Outline',
            visible: true,
            focused: false
        }, options, legend);

        if (request.probe_points.length > 0) {
            let legend_entry = this.props.ui_state.position_legend().get_or_insert({
                id: 'probe_points',
                name: 'Probe Points',
                color: '#34ebeb',
                visible: true,
                focused: false
            });
            legend.push(legend_entry);

            if (legend_entry.visible) {
                for (var i = 0; i < request.probe_points.length; i++) {
                    options.entities.push({
                        kind: EntityKind.Circle,
                        color: '#34ebeb',
                        radius: 4,
                        center: request.probe_points[i]
                    });
                }
            }
        }

        return true;
    }

    _on_click = (pt: Point) => {
        let ctx = this.props.context;

        run_machine_command(ctx, this.props.machine, {
            goto: {
                // TODO: Pull the feed rate from the other ui input.
                feed_rate: 1000,
                x: pt.x,
                y: pt.y,
            }

        }, () => { });
    }

    _get_figure_element() {
        if (!this.state._ready) {
            return <span>Loading...</span>;
        }

        let [options, legend] = this._get_figure_options();
        // TODO: Clean up any unused entries in the ui_state.position_legend FigureLegendState that are no longer being used.

        // TODO: When hovering over an object bounding box, highlight that object.

        return (
            <>
                <Figure options={options} onClick={this._on_click} />
                <FigureLegend entries={legend} onChange={(entry) => {
                    this.props.ui_state.position_legend().set(entry);
                }} />
                {this._get_layers_slider()}
            </>
        )
    }

    _get_layer_data(): LayerData | undefined {
        let machine = this.props.machine;
        if (!machine.state.loaded_program) {
            return;
        }

        let preview = machine.state.loaded_program.preview;
        if (!preview || !preview.state.ready) {
            return;
        }

        if (!preview.layers || preview.layers.length == 0) {
            return;
        }

        let current_index = 0;

        let layer_groups: LayerGroup[] = [];
        preview.layers.map((l, i) => {
            let z = l.z || 0;
            if (layer_groups.length == 0 || z != layer_groups[layer_groups.length - 1]) {
                layer_groups.push({
                    start_index: i,
                    end_index: i,
                    z: z
                });
            }

            if (machine.state.running_program) {
                // Index of the next line to execute.
                let line_number = (machine.state.running_program.line_number || 0) + 1;

                if (line_number >= (l.start_line || 0)) {
                    current_index = layer_groups.length - 1;
                }
            }

            layer_groups[layer_groups.length - 1].end_index = i + 1;
        });

        let user_overriden = false;
        if (this.state._layer_index !== null && this.state._layer_index < layer_groups.length) {
            current_index = this.state._layer_index;
            user_overriden = true;
        }

        return {
            current_index,
            layer_groups,
            user_overriden
        };
    }

    _get_layers_slider() {
        let data = this._get_layer_data();
        if (!data) {
            return;
        }

        return (
            <div style={{ display: "flex", borderTop: '1px solid #ccc', alignItems: 'center', paddingTop: 10, marginTop: 10 }}>
                {min_width_div(data.layer_groups, data.current_index, (layer_group, i) => {
                    return `Layer ${i + 1} / ${data.layer_groups.length}`;
                })}
                <div style={{ flexGrow: 1, padding: '3px 10px 0 10px' }}>
                    <input style={{ width: '100%' }} type="range"
                        min={0}
                        max={data.layer_groups.length - 1}
                        step={1}
                        value={data.current_index}
                        onChange={(e) => {
                            this.setState({ _layer_index: e.target.value * 1 });
                        }}
                    />
                </div>
                {min_width_div(data.layer_groups, data.current_index, (layer_group, _) => {
                    return `Z = ${layer_group.z}`;
                })}
                <div style={{ paddingLeft: 5 }}>
                    <Button preset="light" onClick={(done) => {
                        if (this.state._layer_index !== null) {
                            this.setState({ _layer_index: null })
                        } else {
                            this.setState({ _layer_index: data.current_index });
                        }

                        done();
                    }}>
                        <span className="material-symbols-outlined" style={{ paddingRight: 5, verticalAlign: 'bottom' }}>
                            {data.user_overriden ? 'check_box_outline_blank' : 'check_box'}
                        </span>
                        Current
                    </Button>
                </div>
            </div>
        );
    }


    render() {
        return (
            <Card id="pos" header="Top-down View" style={{ marginBottom: 10 }}>
                <CardBody>
                    <div style={{ textAlign: 'center' }}>
                        {this._get_figure_element()}
                    </div>
                </CardBody>
            </Card>
        );
    }
};

interface LayerData {
    // Index into layer_groups
    current_index: number;

    // Each of these 
    layer_groups: LayerGroup[];

    user_overriden: boolean;
}

interface LayerGroup {
    start_index: number;
    end_index: number;
    z: number;
}


