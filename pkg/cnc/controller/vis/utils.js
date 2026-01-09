

export class Timeline {
    constructor() {
        this._objects = [];
        this._key_frames = [];
        this._duration = 0.0;
        this._start_time = 0.0;
        this._name = '';
    }

    name() {
        return this._name;
    }

    set_name(name) {
        this._name = name;
    }

    set_start_time(v) {
        this._start_time = v;
    }

    set_duration(v) {
        this._duration = v;
    }

    duration() {
        return this._duration - this._start_time;
    }

    add_object(name, params, draw) {
        // Adding some built in params.
        let all_params = {
            opacity: 0
        };

        for (const [key, value] of Object.entries(params)) {
            all_params[key] = value;
        }

        this._objects.push({
            name,
            params: all_params,
            draw
        });
    }

    add_key_frame(object, time, params) {
        if (object instanceof Array) {
            for (var i = 0; i < object.length; i++) {
                this.add_key_frame(object[i], time, params);
            }
            return;
        }

        this._key_frames.push({
            object,
            time,
            params,
            original_index: this._key_frames.length
        });
    }

    // NOTE: The assumption with this is that all key frames ending at start_time that
    // involve the animated params have already been added.
    add_transition(object, start_time, duration, end_params) {
        if (object instanceof Array) {
            for (var i = 0; i < object.length; i++) {
                this.add_transition(object[i], start_time, duration, end_params);
            }
            return;
        }

        let end_time = start_time + duration;

        let all_start_params = this._calculate_params(start_time)[object].values;

        let start_params = {};
        for (const [key, _] of Object.entries(end_params)) {
            if (!all_start_params.hasOwnProperty(key)) {
                throw new Error("Param has no initial value: " + key);
            }

            start_params[key] = all_start_params[key];
        }

        this.add_key_frame(object, start_time, start_params);
        this.add_key_frame(object, end_time, end_params);
    }

    draw(canvas, ctx, time) {
        time = time + this._start_time;

        // Clear the entire canvas
        ctx.clearRect(0, 0, canvas.width, canvas.height);

        // Make canvas white
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(0, 0, canvas.width, canvas.height);

        // Calculate param values.
        let object_params = this._calculate_params(time);

        for (var i = 0; i < this._objects.length; i++) {
            let object = this._objects[i];
            let params = object_params[object.name].values;

            if (params.opacity == 0) {
                continue;
            }

            ctx.save();
            ctx.globalAlpha = params.opacity;
            object.draw(ctx, params);
            ctx.restore();
        }
    }

    _calculate_params(time) {
        // Map from object name to param objects.
        let object_params = {};

        for (var i = 0; i < this._objects.length; i++) {
            // The time of the key frame used for each parameter for interpolation.
            let times = {};
            for (const [key, _] of Object.entries(this._objects[i].params)) {
                times[key] = 0;
            }

            object_params[this._objects[i].name] = {
                values: shallow_copy(this._objects[i].params),
                times
            };
        }

        for (var i = 0; i < this._key_frames.length; i++) {
            let key_frame = this._key_frames[i];
            let params = object_params[key_frame.object];

            for (const [param_key, param_value] of Object.entries(key_frame.params)) {
                if (time <= params.times[param_key]) {
                    continue;
                }

                if (time >= key_frame.time) {
                    params.values[param_key] = param_value;
                    params.times[param_key] = key_frame.time;
                    continue;
                }

                let last_time = params.times[param_key];
                let last_value = params.values[param_key];

                // Linear interpolation.
                if (typeof (param_value) == 'number' && typeof (last_value) == 'number') {
                    let percent = (time - last_time) / (key_frame.time - last_time);
                    params.values[param_key] = last_value + (param_value - last_value) * percent;
                    params.times[param_key] = key_frame.time;
                }
            }
        }

        return object_params;
    }


    start(canvas, ctx) {
        this._key_frames.sort((a, b) => {
            if (a.time == b.time) {
                return a.original_index - b.original_index;
            }

            return a.time - b.time;
        });
        this.draw_loop(canvas, ctx, new Date());
    }

    draw_loop(canvas, ctx, start_time) {
        let now = new Date();
        let t = (now - start_time) / 1000;
        this.draw(canvas, ctx, t);
        requestAnimationFrame(() => this.draw_loop(canvas, ctx, start_time));
    }
}

export class AnimationBuilder {

    constructor(timeline) {
        this._timeline = timeline;
    }

    pause(t) {


    }

}


export function shallow_copy(object) {
    let out = {};
    Object.assign(out, object);
    return out;
}

export function draw_title(ctx, text) {
    ctx.fillStyle = '#000000';
    ctx.lineWidth = 1;
    ctx.font = '30px "Noto Sans"';
    ctx.fillText(text, 30, 60);
}

export function deg2rad(v) {
    return (3.14159 / 180) * v;
}

// Draws a box centered at (0,0) with a fill and outer stroke.
export function draw_box(ctx, width, height) {
    ctx.fillRect(
        -(width / 2),
        -(height / 2),
        width,
        height
    );
    ctx.strokeRect(
        -(width / 2),
        -(height / 2),
        width,
        height
    );
}

export function draw_box_text(ctx, width, height, text, text_color) {
    ctx.save();

    draw_box(ctx, width, height);

    ctx.fillStyle = '#000';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    if (text_color) {
        ctx.fillStyle = text_color;
    }
    ctx.fillText(text, 0, 0);

    ctx.restore();
}

export function draw_multiline_text(ctx, params) {

    let font_size = params.font_size || 25;
    let font_family = params.font_family || 'Noto Sans';
    let color = params.color || '#000';
    let text = params.text;
    let text_align = params.text_align || 'center';

    ctx.font = `${font_size}px "${font_family}"`;

    ctx.fillStyle = color;
    ctx.textAlign = text_align;
    ctx.textBaseline = 'middle';

    let lines = text.split('\n');

    let line_height = font_size * 1.2;

    let start_y = -((lines.length - 1) * line_height) / 2;
    lines.forEach((line, i) => {
        ctx.fillText(line, 0, start_y + (i * line_height));
    });
}

export class DiagramBox {
    constructor(params) {
        this._width = params.width;
        this._height = params.height;
        this._text = params.text || '';
        this._font_size = params.font_size || 25;
        this._font_family = params.font_family || 'Noto Sans';
        this._text_color = params.text_color || '#000';
        this._position = params.position || { x: 0, y: 0 }
        this._text_offset = params.text_offset || { x: 0, y: 0 };
        this._background_color = params.background_color || '#aaccee';
    }

    set_background_color(color) {
        this._background_color = color;
    }

    position() {
        return this._position;
    }

    set_text(text) {
        this._text = text;
    }

    set_text_color(color) {
        this._text_color = color;
    }

    draw(ctx) {
        ctx.save();

        ctx.translate(this._position.x, this._position.y);

        ctx.fillStyle = this._background_color;
        ctx.strokeStyle = '#000'

        draw_box(ctx, this._width, this._height);

        ctx.translate(this._text_offset.x, this._text_offset.y);

        draw_multiline_text(ctx, {
            text: this._text,
            font_size: this._font_size,
            font_family: this._font_family,
            color: this._text_color
        });

        ctx.restore();
    }

    top_center() {
        return {
            x: this._position.x,
            y: this._position.y - (this._height / 2)
        }
    }

    bottom_center() {
        return {
            x: this._position.x,
            y: this._position.y + (this._height / 2)
        }
    }

    right_center() {
        return {
            x: this._position.x + (this._width / 2),
            y: this._position.y
        }
    }

    left_center() {
        return {
            x: this._position.x - (this._width / 2),
            y: this._position.y
        }
    }
}

export function slide_body_grid(canvas) {
    // Placing the grid below the title with a 30 pixel margin on all sides.

    let margin = 30;
    let title_bottom = 60;

    return new Grid({
        left: margin,
        width: canvas.width - 2 * margin,
        top: title_bottom + margin,
        height: canvas.height - title_bottom - 2 * margin
    });
}

export class Grid {
    constructor(params) {
        this._left = params.left;
        this._top = params.top;
        this._width = params.width;
        this._height = params.height;
        this._rows = params.rows;
        this._cols = params.cols;
    }

    center() {
        return {
            x: this._left + (this._width / 2),
            y: this._top + (this._height / 2)
        };
    }

    top_center() {
        return {
            x: this._left + (this._width / 2),
            y: this._top
        };
    }

    bottom_center() {
        return {
            x: this._left + (this._width / 2),
            y: this._top + this._height
        }
    }

    left_center() {
        return {
            x: this._left,
            y: this.center().y
        }
    }

    right_center() {
        return {
            x: this._left + this._width,
            y: this.center().y
        }
    }

    bottom_left() {
        return {
            x: this._left,
            y: this._top + this._height
        }
    }

    width() {
        return this._width;
    }

    height() {
        return this._height;
    }

    cell(row, col) {
        let width = this._width / this._cols;
        let height = this._height / this._rows;

        let left = this._left + (width * col);
        let top = this._top + (height * row);

        return new Grid({
            left,
            top,
            width,
            height,
            rows: 1,
            cols: 1
        });
    }

    split(rows, cols) {
        return new Grid({
            left: this._left,
            top: this._top,
            width: this._width,
            height: this._height,
            rows,
            cols
        });
    }

}

/*
Wires have a
- 'height' : Default 0

- List of [{ high: true, time: 0 }] values (first is always at 0 and last is always at 1)
*/

export class Wire {
    constructor(params) {
        this.height = 0;
        this.title = params.title || '';
        this.line_width = params.line_width || 2;
        this.color = '#000';
        this.graph = [];
    }

}

export class WireBundle {

    constructor(params) {
        this._wires = [];
        this._from = params.from;
        this._to = params.to;
        this._spacing = params.spacing;
    }

    wires() {
        return this._wires;
    }

    set_to(pos) {
        this._to = pos;
    }

    add_wire(wire) {
        this._wires.push(wire);
    }

    draw(ctx) {

        let height = (this._wires.length - 1) * this._spacing;
        this._wires.map((wire, i) => {
            height += wire.height;
        })

        let current_y = this._from.y - (height / 2);

        this._wires.map((wire, i) => {

            if (wire.title) {

                ctx.save();
                ctx.fillStyle = '#000';
                ctx.font = `18px "Noto Sans"`;

                ctx.textAlign = 'left';
                ctx.textBaseline = 'bottom';

                ctx.fillText(wire.title, this._from.x + 8, current_y - wire.line_width);

                ctx.restore();
            }

            if (wire.height >= wire.line_width) {
                let width = this._to.x - this._from.x;

                ctx.fillStyle = '#888';
                ctx.fillRect(this._from.x, current_y, width, wire.height);

                ctx.save();

                ctx.beginPath();
                ctx.rect(this._from.x, current_y, width, wire.height);
                ctx.clip();

                ctx.beginPath();
                wire.graph.map((point, i) => {
                    let margin = 5;

                    let inner_height = wire.height - 2 * margin;

                    let x = this._from.x + (width * point.x);
                    let y = current_y + wire.height - margin - (inner_height * point.y);

                    if (i == 0) {
                        ctx.moveTo(x, y);
                    } else {
                        ctx.lineTo(x, y);
                    }
                });

                ctx.strokeStyle = '#fff';
                ctx.stroke();

                ctx.restore();

                ctx.lineWidth = 2;
                ctx.strokeStyle = '#000';
                ctx.strokeRect(this._from.x, current_y, width, wire.height);


            } else {
                ctx.beginPath();
                ctx.moveTo(this._from.x, current_y);
                ctx.lineTo(this._to.x, current_y);

                ctx.lineWidth = wire.line_width;
                ctx.strokeStyle = wire.color;
                ctx.stroke();
            }


            current_y += this._spacing + wire.height;
        });
    }



}


