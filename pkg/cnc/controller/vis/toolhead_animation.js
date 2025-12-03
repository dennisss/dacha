import { Timeline, draw_title, deg2rad, draw_box } from './utils.js';
import { drawArrow } from './arrow.js';
import { getRayToRectIntersection } from './box_angle.js';
import { Gear } from './gear.js';

/*
cd pkg/cnc/controller/vis/
python3 -m http.server 9000

- Output: 3840 x 2160
- Display: 960 x 540
*/






function graph(ctx, color, fx) {

    let width = 150;
    let height = 50;

    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(width, 0);

    ctx.moveTo(0, 1.5);
    ctx.lineTo(0, -height - 3);

    ctx.strokeStyle = '#666';
    ctx.lineWidth = 3;
    ctx.stroke();

    ctx.lineWidth = 1;
    ctx.strokeStyle = color;

    let num_steps = 400;

    ctx.beginPath();
    for (var i = 0; i < num_steps; i++) {
        let x = i / num_steps;

        let y = fx(x)
        if (y === null || y === undefined) {
            break;
        }

        let x_pos = width * x + 3;
        let y_pos = -height * y - 3;

        if (i == 0) {
            ctx.moveTo(x_pos, y_pos);
        } else {
            ctx.lineTo(x_pos, y_pos)
        }
    }

    ctx.stroke();
}

export const BACKGROUND_MODE = 'background';
export const PRESENCE_MODE = 'presence';
export const CONTROL_MODE = 'control';
export const MODEL_MODE = 'model';
export const PLAN_MODE = 'plan';
export const PZ_MODE = 'pz';

function model_mode(canvas) {

    let vid = new Timeline();

    vid.add_object('title', { opacity: 0 }, (ctx) => {
        draw_title(ctx, 'Tool Head Thermal Model');
    });

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    let grid_width = canvas.width - 300;
    let grid_height = canvas.height - 250;

    let grid_cols = 3;
    let grid_rows = 3;

    let grid_center_x = centerX;
    let grid_center_y = centerY + 40;

    function grid_pos(r, c) {
        let r_unit = grid_height / (grid_rows - 1);
        let c_unit = grid_width / (grid_cols - 1);

        c -= (grid_cols - 1) / 2;
        r -= (grid_rows - 1) / 2;

        return {
            x: grid_center_x + c_unit * c,
            y: grid_center_y + r_unit * r,
        }
    }

    let box_width = 200;
    let box_height = 100;

    function model_box_edge_pos(r, c, angle) {
        let base = grid_pos(r, c);
        let delta = getRayToRectIntersection(box_width, box_height, angle);
        return {
            x: base.x + delta.x,
            y: base.y + delta.y
        }
    }


    function model_box(ctx, r, c, text) {
        ctx.save();

        let pos = grid_pos(r, c);
        ctx.translate(pos.x, pos.y);

        ctx.fillStyle = '#aaccee';
        ctx.strokeStyle = '#000'
        ctx.font = '25px "Noto Sans"';

        draw_box_text(ctx, box_width, box_height, text, '#000');

        ctx.restore();
    }


    function draw_arrow(ctx, r1, c1, an1, r2, c2, an2, rev = false) {
        let a1 = model_box_edge_pos(r1, c1, deg2rad(an1));
        let a2 = model_box_edge_pos(r2, c2, deg2rad(an2));
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        drawArrow(ctx, a1.x, a1.y, a2.x, a2.y, 2, 20, rev);

        return { x: (a1.x + a2.x) / 2, y: (a1.y + a2.y) / 2 }
    }


    vid.add_object('heater', { opacity: 0 }, (ctx) => {
        model_box(ctx, 1, 0, 'Heater');


    });

    vid.add_object('heater_arrow', { opacity: 0 }, (ctx) => {
        let p = draw_arrow(ctx, 1, 0, 0, 1, 1, 180);
        {
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'bottom';
            ctx.font = '18px "Noto Sans"';
            ctx.fillText('A * DutyCycle * f(temp)', p.x, p.y - 50);
        }
    })


    vid.add_object('heater_block', { opacity: 0 }, (ctx) => {
        model_box(ctx, 1, 1, 'Heater Block');
    });

    vid.add_object('heater_block_arrow', { opacity: 0 }, (ctx) => {
        let p = draw_arrow(ctx, 1, 1, 0, 1, 2, 180, true);

        {
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'bottom';
            ctx.font = '18px "Noto Sans"';
            ctx.fillText('B * ΔT', p.x, p.y);
        }
    })

    vid.add_object('nozzle', { opacity: 0 }, (ctx) => {
        model_box(ctx, 1, 2, 'Nozzle');
    });

    vid.add_object('air', { opacity: 0 }, (ctx) => {
        model_box(ctx, 0, 1, 'Air');
    });

    vid.add_object('air_arrow', { opacity: 0 }, (ctx) => {
        let p = draw_arrow(ctx, 1, 1, -90, 0, 1, 90);
        draw_arrow(ctx, 1, 2, -90 - 45, 0, 1, 45);

        {
            ctx.fillStyle = '#000';
            ctx.textAlign = 'left';
            ctx.textBaseline = 'middle';
            ctx.font = '18px "Noto Sans"';
            ctx.fillText('C * ΔT', p.x + 15, p.y);
        }
    })

    vid.add_object('fan', { opacity: 0 }, (ctx) => {
        model_box(ctx, 0, 2, 'Fan');
    });

    vid.add_object('fan_arrow', { opacity: 0 }, (ctx) => {
        let p = draw_arrow(ctx, 1, 2, -90, 0, 2, 90);

        {
            ctx.fillStyle = '#000';
            ctx.textAlign = 'left';
            ctx.textBaseline = 'middle';
            ctx.font = '18px "Noto Sans"';
            ctx.fillText('D * Speed * ΔT', p.x + 15, p.y);
        }
    });

    vid.add_object('filament', { opacity: 0 }, (ctx) => {
        model_box(ctx, 2, 2, 'Filament');

        draw_arrow(ctx, 1, 2, 90, 2, 2, -90, true);
    });

    vid.add_object('bed', { opacity: 0 }, (ctx) => {
        model_box(ctx, 2, 1, 'Bed');

        draw_arrow(ctx, 2, 2, 180, 2, 1, 0, false);
    });

    {
        let t = 0;

        // Fade in initial stuff
        let bg_objs = ['title', 'heater', 'heater_block', 'nozzle', 'air', 'fan'];
        vid.add_key_frame(bg_objs, t + 0.5, { opacity: 1 });
        t += 0.5;

        t += 1;

        ['heater_arrow', 'heater_block_arrow', 'air_arrow', 'fan_arrow'].map((obj) => {

            vid.add_key_frame(obj, t, { opacity: 0 });
            vid.add_key_frame(obj, t + 0.5, { opacity: 1 });
            t += 0.5;

            t += 1;
        });

        vid.add_key_frame(['filament', 'bed'], t, { opacity: 0 });
        vid.add_key_frame(['filament', 'bed'], t + 0.5, { opacity: 0.2 });
        t += 0.5;

        t += 1;

        vid.set_duration(t);
    }

    return vid;
}


function plan_mode(canvas) {

    let vid = new Timeline();

    vid.add_object('title', { opacity: 1 }, (ctx) => {
        draw_title(ctx, 'The Plan');
    });

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;


    function point(ctx, i, text) {
        ctx.fillStyle = '#000';
        ctx.textAlign = 'left';
        // ctx.textBaseline = 'middle';
        ctx.font = '25px "Noto Sans"';
        ctx.fillText(text, 30, 150 + i * 100);
    }


    vid.add_object('1', { opacity: 0 }, (ctx) => {
        point(ctx, 0, '1. Collect tool head performance data.');
    });
    vid.add_object('2', { opacity: 0 }, (ctx) => {
        point(ctx, 1, '2. Build a simulation model.');
    });
    vid.add_object('3', { opacity: 0 }, (ctx) => {
        point(ctx, 2, '3. Find heater inputs using the simulation.');
    });

    let t = 0;
    t += 1;

    for (var i = 0; i < 3; i++) {
        vid.add_key_frame((i + 1) + '', t, { opacity: 0 });
        vid.add_key_frame((i + 1) + '', t + 0.5, { opacity: 1 });
        t += 0.5;
        t += 1;
    }



    vid.set_duration(t);

    return vid;
}




function pz_mode(canvas) {

    let vid = new Timeline();

    vid.add_object('title', { opacity: 0 }, (ctx) => {
        draw_title(ctx, 'PZ Probe Signal Processing');
    });

    let noise = [];
    for (var i = 0; i < 400; i++) {
        noise.push(Math.random());
    }

    vid.add_object('graph', { opacity: 0, scan: 0, threshold: 0 }, (ctx, params) => {
        let width = (canvas.width / 2) - 100;
        let height = canvas.height - 200;

        ctx.translate(40, height + 150);

        // Title
        {
            ctx.font = '20px "Noto Sans"';
            ctx.fillStyle = '#000';
            ctx.fillText('Sensor Output', 0, - height - 20)
        }

        {
            ctx.font = '16px "Noto Sans"';
            ctx.fillStyle = '#000';
            ctx.fillText('1V', -30, - height + 16)
        }

        {
            ctx.font = '16px "Noto Sans"';
            ctx.fillStyle = '#000';
            ctx.textAlign = 'right';
            ctx.fillText('100ms', width, 30)
        }

        // Axes
        {
            ctx.beginPath();
            ctx.moveTo(0, 0);
            ctx.lineTo(width, 0);

            ctx.moveTo(0, 1.5);
            ctx.lineTo(0, -height - 3);

            ctx.strokeStyle = '#666';
            ctx.lineWidth = 3;
            ctx.stroke();
        }

        // Threshold
        {
            ctx.save();

            ctx.beginPath();
            ctx.moveTo(0, -height / 2);
            ctx.lineTo(width, -height / 2);

            ctx.setLineDash([5, 5]);
            ctx.strokeStyle = `rgba(255, 0, 0, ${params.threshold})`;
            ctx.lineWidth = 1;

            ctx.stroke();


            ctx.restore();
        }

        ctx.lineWidth = 1;
        ctx.strokeStyle = '#0bf';

        let fx = (x) => {
            if (x > params.scan) {
                return null;
            }

            let v = noise[Math.floor(x * noise.length)];

            if (x > 0.6 && x < 0.61) {
                return v * 0.1 + 0.6;
            }


            return v * 0.1;
        };

        let num_steps = 400;

        ctx.beginPath();
        for (var i = 0; i < num_steps; i++) {
            let x = i / num_steps;

            let y = fx(x)
            if (y === null || y === undefined) {
                break;
            }

            let x_pos = width * x + 3;
            let y_pos = -height * y - 3;

            if (i == 0) {
                ctx.moveTo(x_pos, y_pos);
            } else {
                ctx.lineTo(x_pos, y_pos)
            }
        }

        ctx.stroke();

    });


    let grid_top = 130;
    let grid_bottom = canvas.height - 20;

    let grid_left = (canvas.width / 2);
    let grid_right = (canvas.width);

    let grid_rows = 3;
    let grid_cols = 2;

    let grid_width = (grid_right - grid_left);
    let grid_height = grid_bottom - grid_top;

    let box_width = 180;
    let box_height = 90;

    function grid_pos(r, c) {
        let r_unit = grid_height / grid_rows;
        let c_unit = grid_width / grid_cols;

        return {
            x: grid_left + c_unit * (c + 0.5),
            y: grid_top + r_unit * (r + 0.5),
        }
    }

    function model_box(ctx, r, c, text, alpha) {
        ctx.save();

        let pos = grid_pos(r, c);
        ctx.translate(pos.x, pos.y);

        // 170, 204, 238

        // ctx.fillStyle = '#aaccee';
        ctx.fillStyle = `rgba(170, 204, 238, ${alpha})`
        ctx.strokeStyle = '#000'
        ctx.font = '25px "Noto Sans"';

        draw_box_text(ctx, box_width, box_height, text, '#000');

        ctx.restore();
    }

    function model_box_edge_pos(r, c, angle) {
        let base = grid_pos(r, c);
        let delta = getRayToRectIntersection(box_width, box_height, angle);
        return {
            x: base.x + delta.x,
            y: base.y + delta.y
        }
    }

    function draw_arrow(ctx, r1, c1, an1, r2, c2, an2, alpha) {
        let a1 = model_box_edge_pos(r1, c1, deg2rad(an1));
        let a2 = model_box_edge_pos(r2, c2, deg2rad(an2));

        let c = `rgba(0,0,0,${alpha})`;
        ctx.fillStyle = c;
        ctx.strokeStyle = c;
        drawArrow(ctx, a1.x, a1.y, a2.x, a2.y, 2, 20, false);

        return { x: (a1.x + a2.x) / 2, y: (a1.y + a2.y) / 2 }
    }

    let sleeping_alpha = 0.1;

    function get_alpha(v) {
        return sleeping_alpha + (Math.sin(v * Math.PI / 2)) * (1 - sleeping_alpha);
    }

    vid.add_object('adc', { opacity: 0, alpha: 0 }, (ctx, params) => {
        let a = get_alpha(params.alpha);
        model_box(ctx, 1, 0, 'ADC', a);
        draw_arrow(ctx, 1, -1, 0, 1, 0, 180, a);
        draw_arrow(ctx, 1, 0, -90, 0, 0, 90, a);
    });
    vid.add_object('timer', { opacity: 0, alpha: 0 }, (ctx, params) => {
        let a = get_alpha(params.alpha);
        model_box(ctx, 2, 0, '2kHz Timer', a);
        draw_arrow(ctx, 2, 0, -90, 1, 0, 90, a);
    });
    vid.add_object('comp', { opacity: 0, alpha: 0 }, (ctx, params) => {
        let a = get_alpha(params.alpha);
        model_box(ctx, 0, 0, 'Input > 0.5V', a);
        draw_arrow(ctx, 0, 0, 0, 0, 1, 180, a);
    });
    vid.add_object('cpu', { opacity: 0, alpha: 0 }, (ctx, params) => {
        let a = get_alpha(params.alpha);

        let text = 'CPU  ';
        let emot = '😴';
        if (params.alpha > 0) {
            text = 'CPU!!  ';
            emot = '😨';
        }

        model_box(ctx, 0, 1, text, a);

        let p = grid_pos(0, 1);

        ctx.fillStyle = '#000';
        ctx.textDrawingMode = "glyph";
        ctx.font = '25px "Noto Color Emoji"';

        ctx.fillText(emot, p.x + 30, p.y + 8);

    });

    let scan_dur = 10;

    let sample_count = 10;
    let sample_interval = scan_dur / sample_count;
    let sample_dur = 0.4;



    let t = 0;


    {
        vid.add_transition(['title', 'graph'], t, 0.5, { opacity: 1 });
        t + 0.5;
    }

    t += 1;


    {
        vid.add_transition('graph', t, 4, { scan: 1 });
        t += 4;
    }

    t += 1;

    {
        vid.add_key_frame('graph', t, { threshold: 0 });
        vid.add_key_frame('graph', t + 0.5, { threshold: 1 });
        t += 0.5;
    }

    t += 1;

    {
        vid.add_key_frame('adc', t, { opacity: 0 });
        vid.add_key_frame('adc', t + 0.5, { opacity: 1 });
        t += 0.5;
    }

    t += 1;

    {
        vid.add_key_frame('timer', t, { opacity: 0 });
        vid.add_key_frame('timer', t + 0.5, { opacity: 1 });
        t += 0.5;
    }

    t += 1;

    {
        vid.add_key_frame('comp', t, { opacity: 0 });
        vid.add_key_frame('comp', t + 0.5, { opacity: 1 });
        vid.add_key_frame('cpu', t, { opacity: 0 });
        vid.add_key_frame('cpu', t + 0.5, { opacity: 1 });
        t += 0.5;
    }

    t += 1;

    vid.add_key_frame('graph', t - 0.1, { scan: 1 });
    vid.add_key_frame('graph', t, { scan: 0 });
    vid.add_key_frame('graph', t + scan_dur, { scan: 1 });

    for (var i = 0; i < sample_count; i++) {
        vid.add_key_frame('timer', t, { alpha: 0 });
        vid.add_key_frame('timer', t + sample_dur, { alpha: 1 });
        vid.add_key_frame('timer', t + 2 * sample_dur, { alpha: 0 });

        vid.add_key_frame('adc', t + 0.1, { alpha: 0 });
        vid.add_key_frame('adc', t + 0.1 + sample_dur, { alpha: 1 });
        vid.add_key_frame('adc', t + 0.1 + 2 * sample_dur, { alpha: 0 });

        let x = (i / sample_count);
        if (Math.abs(x - 0.6) < 0.01) {
            vid.add_key_frame('comp', t + 0.2, { alpha: 0 });
            vid.add_key_frame('comp', t + 0.2 + sample_dur, { alpha: 1 });
            vid.add_key_frame('comp', t + 0.2 + 2 * sample_dur, { alpha: 0 });

            vid.add_key_frame('cpu', t + 0.3, { alpha: 0 });
            vid.add_key_frame('cpu', t + 0.3 + sample_dur, { alpha: 1 });
            // vid.add_key_frame('cpu', t + 0.3, { alpha: 0 });
            // vid.add_key_frame('cpu', t + 0.3 + sample_dur, { alpha: 1 });
        }

        t += sample_interval;
    }




    vid.set_duration(t);

    return vid;
}



export function configure(canvas, mode) {

    if (!mode) {
        mode = PZ_MODE;
    }

    let vid = new Timeline();

    let title = '??';
    if (mode == BACKGROUND_MODE) {
        title = 'Tool Head Internals';
    } else if (mode == PRESENCE_MODE) {
        title = 'Filament Presence Sensing'
    } else if (mode == CONTROL_MODE) {
        title = 'Heater Control'
    } else if (mode == MODEL_MODE) {
        return model_mode(canvas);
    } else if (mode == PLAN_MODE) {
        return plan_mode(canvas);
    } else if (mode == PZ_MODE) {
        return pz_mode(canvas);
    }

    vid.add_object('title', { opacity: 0 }, (ctx) => {
        draw_title(ctx, title);
    });


    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    let body_width = 250;
    let body_height = 200;

    let body_top = centerY - (body_height / 2) - 50;

    let body_middle = body_top + (body_height / 2);
    let body_bottom = body_top + body_height;

    let filament_width = 15;

    let extruder_grip = 4;

    // Distance from the heatsink to the heater.
    let heat_gap = 20;

    let heater_width = 150;
    let heater_height = 40;
    let heater_top = body_bottom + heat_gap;
    let heater_bottom = heater_top + heater_height;

    let channel_width = 20;

    let heatsink_width = 150;
    let heatsink_num_fins = 5;
    let heatsink_fin_gap = 10;
    let heatsink_margin = 10;

    let nozzle_upper_width = 100;
    let nozzle_lower_width = 40;
    let nozzle_tip_width = channel_width / 4;

    let nozzle_upper_height = 10;
    let nozzle_lower_height = 30;

    let nozzle_top = heater_bottom;
    let nozzle_bottom = nozzle_top + nozzle_upper_height + nozzle_lower_height;

    let filament_color = '#0af';


    vid.add_object('body', {}, (ctx) => {
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;
        ctx.fillStyle = '#ddd';

        ctx.translate(centerX, body_middle);
        draw_box(ctx, body_width, body_height);
    });

    vid.add_object('heater', {}, (ctx) => {
        function cable(i, color, width) {
            ctx.save();
            ctx.lineWidth = width;
            ctx.strokeStyle = color;
            ctx.beginPath();
            ctx.moveTo(centerX - (heater_width / 2), heater_top + i * (heater_height / 6));
            ctx.lineTo(centerX - 450, heater_top + i * (heater_height / 6));
            ctx.stroke();
            ctx.restore();
        }

        cable(1, '#f00', 4);
        cable(2, '#000', 4);

        cable(4, '#000', 2);
        cable(5, '#000', 2);

        ctx.fillStyle = '#f44';
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;

        ctx.save();
        ctx.translate(centerX, heater_top + (heater_height / 2));
        draw_box(ctx, heater_width, heater_height);
        ctx.restore();

        let thermistor_width = 20;
        let thermistor_height = 10;

        ctx.save();
        ctx.fillStyle = '#ddd';
        ctx.translate(
            centerX - (heater_width / 2) + (thermistor_width / 2),
            heater_top + 4.5 * (heater_height / 6)
        );
        draw_box(ctx, thermistor_width, thermistor_height);
        ctx.restore();
    });

    let random_numbers = [];
    for (var i = 0; i < 21; i++) {
        random_numbers.push(Math.random());

    }

    vid.add_object('heater_csense', { opacity: 0, t: 0 }, (ctx, params) => {

        let wire_y = heater_top + 1 * (heater_height / 6);

        ctx.translate(centerX - (heater_width / 2) - 120, wire_y + 2);

        let gap_width = 20;

        ctx.fillStyle = '#fff';
        ctx.fillRect(-(gap_width / 2), 0, gap_width, -100);

        ctx.fillStyle = '#f00';
        ctx.fillRect(-(gap_width / 2), 0, -4, -50);
        ctx.fillRect((gap_width / 2), 0, 4, -50);

        let box_width = 60;
        let box_height = 40

        ctx.fillStyle = '#ddd';
        ctx.strokeStlye = '#000';
        ctx.translate(0, -50 - (box_height / 2));
        ctx.font = '20px "Noto Sans"';

        let v = Math.round(50 + random_numbers[Math.floor(params.t * 20)] * 20);

        draw_box_text(ctx, box_width, box_height, `${v}W`, '#000');
    });

    vid.add_object('heater_labels', { opacity: 0 }, (ctx) => {
        ctx.font = '20px "Noto Sans"';
        ctx.fillStyle = '#000';
        ctx.fillText('Heater Power', centerX - 450, heater_top);
        ctx.textBaseline = 'top'
        ctx.fillText('Temperature Sensor', centerX - 450, heater_bottom);
    })

    vid.add_object('heater_power_graph', { opacity: 0, limit: 0 }, (ctx, params) => {

        ctx.translate(centerX - 450, heater_top - 30)
        graph(ctx, 'red', (x) => {

            if (x > params.limit) {
                return null;
            }

            x = Math.floor(x * 10) % 2;

            return x;
        });
    });

    vid.add_object('heater_temp_graph', { opacity: 0, limit: 0 }, (ctx, params) => {

        ctx.translate(centerX - 450, heater_bottom + 90)
        graph(ctx, 'green', (x) => {

            if (x > params.limit) {
                return null;
            }

            x = 1 - Math.exp(-7 * x);

            return x;
        });
    });

    vid.add_object('heater_fade', { opacity: 0 }, (ctx) => {
        ctx.translate(centerX - heater_width / 2, heater_top);

        const solidColor = 'rgba(221, 221, 221, 1)';
        const transparentColor = 'rgba(221, 221, 221, 0)';

        const gradient = ctx.createLinearGradient(
            0,
            0,
            heater_width,
            0
        );

        gradient.addColorStop(0, transparentColor);
        gradient.addColorStop(0.3, solidColor);
        gradient.addColorStop(0.7, solidColor);
        gradient.addColorStop(1, transparentColor);

        ctx.fillStyle = gradient;
        ctx.fillRect(0, 0, heater_width, heater_height);
    });

    vid.add_object('fan', {}, (ctx) => {
        ctx.translate(centerX + body_width / 2 + 40, centerY + 80);

        ctx.fillStyle = '#000';
        ctx.font = '20px "Noto Sans"';
        ctx.fillText('Fan', 20, -20);

        ctx.rotate(deg2rad(90 + 45))

        let fan_width = 20;
        let fan_height = 80;
        let num_lines = 4;

        ctx.fillStyle = '#666';
        draw_box(ctx, fan_width, fan_height);

        ctx.beginPath();
        for (var i = 0; i < num_lines; i++) {
            let h = fan_height - 10;

            let y = -(h / 2) + i * (h / (num_lines - 1));


            ctx.moveTo((fan_width / 2) + 5, y);
            ctx.lineTo((fan_width / 2) + 5 + 50, y);
        }
        ctx.stroke();
    });



    vid.add_object('heatsink', {}, (ctx) => {
        ctx.fillStyle = '#fff';
        ctx.fillRect(
            centerX - (heatsink_width / 2) - heatsink_margin,
            body_bottom - ((heatsink_num_fins - 1) * heatsink_fin_gap) - heatsink_margin,
            heatsink_width + (heatsink_margin * 2),
            ((heatsink_num_fins - 1) * heatsink_fin_gap) + (heatsink_margin * 2)
        );

        ctx.strokeStyle = '#c0c0c0';
        ctx.lineWidth = 4;

        for (let i = 0; i < heatsink_num_fins; i++) {
            ctx.beginPath()
            ctx.moveTo(centerX - (heatsink_width / 2), body_bottom - i * heatsink_fin_gap);
            ctx.lineTo(centerX + (heatsink_width / 2), body_bottom - i * heatsink_fin_gap)
            ctx.stroke();
        }
    });

    vid.add_object('center_channel', {}, (ctx) => {
        ctx.fillStyle = '#fff';
        ctx.fillRect(
            centerX - (channel_width / 2),
            body_top - 10,
            channel_width,
            (heater_bottom - body_top) + 20
        );

        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(
            centerX - (channel_width / 2),
            body_top
        );
        ctx.lineTo(
            centerX - (channel_width / 2),
            heater_bottom
        );
        ctx.stroke();

        ctx.beginPath();
        ctx.moveTo(
            centerX + (channel_width / 2),
            body_top
        );
        ctx.lineTo(
            centerX + (channel_width / 2),
            heater_bottom
        );
        ctx.stroke();

    });

    vid.add_object('filament', { bottom: 0 }, (ctx, params) => {
        let filament_height = 1000;

        let full_bottom = Math.min(params.bottom, nozzle_bottom);

        ctx.fillStyle = filament_color;
        ctx.fillRect(
            centerX - (filament_width / 2),
            full_bottom - filament_height,
            filament_width,
            1000
        );

        if (params.bottom > nozzle_bottom) {
            ctx.fillRect(
                centerX - (nozzle_tip_width / 2),
                params.bottom - filament_height,
                nozzle_tip_width,
                1000
            );
        }



    });


    let bed_width = 500;
    let sheet_height = 4;
    let bed_height = 40;
    let layer_height = 6;
    let layer_gap = 1;
    let bed_top = nozzle_bottom + layer_height;


    vid.add_object('bed', { opacity: 1, left: 0, top: 0, layer1_length: 0, layer2_length: 0 }, (ctx, params) => {

        ctx.translate(centerX + params.left, nozzle_bottom + (layer_height / 2) + params.top);

        if (params.layer1_length != 0) {
            ctx.save();
            ctx.lineCap = 'round';
            ctx.lineWidth = layer_height;
            ctx.strokeStyle = filament_color;
            ctx.beginPath();
            ctx.moveTo(0, 0);
            ctx.lineTo(params.layer1_length, 0);
            ctx.stroke();
            ctx.restore();
        }

        if (params.layer2_length != 0) {
            ctx.save();
            ctx.lineCap = 'round';
            ctx.lineWidth = layer_height;
            ctx.strokeStyle = filament_color;
            ctx.beginPath();
            ctx.moveTo(params.layer1_length, -layer_height - layer_gap);
            ctx.lineTo(params.layer1_length + params.layer2_length, -layer_height - layer_gap);
            ctx.stroke();
            ctx.restore();
        }

        ctx.fillStyle = '#444';
        ctx.translate(0, (sheet_height / 2) + (layer_height / 2));
        draw_box(ctx, bed_width, sheet_height);

        ctx.fillStyle = '#ddd';
        ctx.font = '20px "Noto Sans"';
        ctx.translate(0, (sheet_height / 2) + (bed_height / 2));
        draw_box_text(ctx, bed_width, bed_height, 'Bed');

    });

    const extruder_gear = new Gear({
        x: 0,
        y: 0,
        numTeeth: 20,
        module: 3,
        boreRadius: 4,
        fillColor: '#c0c0c0', // Silver
        strokeColor: '#333'
    });

    let gears_y = centerY - 100;
    let lever_length = 80;

    // lever_rotation:
    // - 100 when no filament inserted
    // - 90 when filament inserted
    //
    vid.add_object('gears', {
        lever_rotation: 100,
        gear_rotation: 0,
        opacity: 0,
        magnet_opacity: 0

    }, (ctx, params) => {
        ctx.translate(0, gears_y);

        ctx.save();
        ctx.translate(centerX - (channel_width / 2) - extruder_gear.outerRadius + extruder_grip, 0);
        ctx.rotate(params.gear_rotation);
        extruder_gear.draw(ctx);
        ctx.restore();

        // X chosen that the lever straight up is in line with the left gear
        let lever_x = centerX + (channel_width / 2) + extruder_gear.outerRadius - extruder_grip;
        let lever_y = lever_length;

        let lever_rotation = -deg2rad(params.lever_rotation);

        let right_gear_x = lever_x + lever_length * Math.cos(lever_rotation);
        let right_gear_y = lever_y + lever_length * Math.sin(lever_rotation);

        ctx.save();
        ctx.translate(right_gear_x, right_gear_y);
        ctx.rotate(-params.gear_rotation);
        extruder_gear.draw(ctx);
        ctx.restore();

        // Lever
        {
            ctx.save();
            ctx.translate(lever_x, lever_y);
            ctx.rotate(lever_rotation);

            ctx.save();

            ctx.strokeStyle = 'rgba(68, 68, 68, 0.5)';
            ctx.lineCap = 'round';
            ctx.lineWidth = 10;

            ctx.beginPath();
            ctx.moveTo(0, 0);
            ctx.lineTo(lever_length, 0);
            ctx.stroke();

            ctx.restore();

            // Magnet
            if (params.magnet_opacity > 0) {
                let magnet_width = 30;
                let magnet_height = 15;
                ctx.strokeStyle = `rgba(0,0,0, ${params.magnet_opacity})`;
                ctx.fillStyle = `rgba(238,238,238, ${params.magnet_opacity})`;
                ctx.translate(40 - magnet_width / 2, 5 + magnet_height / 2);
                draw_box(ctx, magnet_width, magnet_height);
            }

            ctx.restore();
        }

        {
            ctx.beginPath();
            ctx.arc(lever_x, lever_y, 6, 0, Math.PI * 2);
            ctx.fillStyle = '#444';
            ctx.fill();
        }

    });

    let hall_sensor_width = 60;
    let hall_sensor_height = 60;

    vid.add_object('hall_highlight', { opacity: 0 }, (ctx, params) => {
        ctx.translate(centerX + body_width / 2, body_top + 100);
        ctx.fillStyle = 'rgba(0,0,0,0)';
        ctx.strokeStyle = 'red';
        ctx.lineWidth = 4;
        draw_box(ctx, 300, 100);
    });

    vid.add_object('hall_highlight2', { opacity: 0 }, (ctx, params) => {
        let h = 30;
        let w = 60;

        ctx.translate(centerX, body_top - h / 2);
        ctx.fillStyle = 'rgba(0,0,0,0)';
        ctx.strokeStyle = 'red';
        ctx.lineWidth = 4;
        draw_box(ctx, w, h);
    });

    vid.add_object('hall_sensor', { opacity: 0, value: 1.5 }, (ctx, params) => {
        ctx.translate(centerX + body_width / 2, body_top + 100);
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        ctx.font = '18px "Noto Sans"';

        let v = Math.round(params.value * 100) / 100;
        draw_box_text(ctx, hall_sensor_width, hall_sensor_height, `${v}V\n`, '#fff');

        ctx.translate(hall_sensor_width / 2 / 2, 0);

        ctx.beginPath();
        for (var i = 0; i < 3; i++) {
            let h = 20;

            let y = (-h / 2) + i * (h / 2)

            ctx.moveTo(0, y);
            ctx.lineTo(100, y);
        }

        ctx.stroke();
    });

    vid.add_object('nozzle', {}, (ctx) => {

        ctx.translate(centerX, nozzle_top);

        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#C68346'; // copper

        ctx.beginPath();
        ctx.moveTo(-nozzle_upper_width / 2, 0);
        ctx.lineTo(-nozzle_upper_width / 2, nozzle_upper_height);
        ctx.lineTo(-nozzle_lower_width / 2, nozzle_upper_height + nozzle_lower_height);
        ctx.lineTo(-nozzle_tip_width / 2, nozzle_upper_height + nozzle_lower_height);
        ctx.lineTo(-channel_width / 2, nozzle_upper_height);
        ctx.lineTo(-channel_width / 2, 0);
        ctx.closePath();

        ctx.fill();
        ctx.stroke();


        // TODO: This is a mirror opposite of above.
        ctx.beginPath();
        ctx.moveTo(nozzle_upper_width / 2, 0);
        ctx.lineTo(nozzle_upper_width / 2, nozzle_upper_height);
        ctx.lineTo(nozzle_lower_width / 2, nozzle_upper_height + nozzle_lower_height);
        ctx.lineTo(nozzle_tip_width / 2, nozzle_upper_height + nozzle_lower_height);
        ctx.lineTo(channel_width / 2, nozzle_upper_height);
        ctx.lineTo(channel_width / 2, 0);
        ctx.closePath();

        ctx.fill();
        ctx.stroke();

    });

    vid.add_object('heater_highlight', {}, (ctx) => {
        ctx.translate(centerX, heater_top + heater_height / 2);
        ctx.fillStyle = 'rgba(0,0,0,0)';
        ctx.strokeStyle = 'red';
        ctx.lineWidth = 4;
        draw_box(ctx, heater_width * 1.5, heater_height * 1.5);

    });

    if (mode == BACKGROUND_MODE) {
        let t = 0;

        // Fade in initial stuff
        vid.add_key_frame('title', t + 0.5, { opacity: 1 });
        vid.add_key_frame('body', t + 0.5, { opacity: 1 });
        t += 0.5;

        // Sit for 1 second.
        t += 1;

        vid.add_key_frame('center_channel', t, { opacity: 0 });
        vid.add_key_frame('center_channel', t + 0.5, { opacity: 1 });
        t += 0.5;

        // Pixels/second
        let filament_move_rate = 100;

        // Rotations per second
        let gear_spin_rate = filament_move_rate / (2 * extruder_gear.outerRadius * Math.PI);
        // Radians per second
        gear_spin_rate = gear_spin_rate * (2 * Math.PI);


        vid.add_key_frame('filament', t, { opacity: 0 });
        vid.add_key_frame('filament', t + 0.01, { opacity: 1 });

        {
            let y = body_top + 26;
            let dur = y / filament_move_rate;
            vid.add_key_frame('filament', t, { bottom: 0 });
            vid.add_key_frame('filament', t + dur, { bottom: y });
            t += dur;
        }

        // Show gears
        {
            vid.add_key_frame('gears', t, { opacity: 0 });
            vid.add_key_frame('gears', t + 0.5, { opacity: 1 });
            t += 0.5;
        }

        let gear_r = 0;

        // Insert filament
        {
            let y = body_top + 26;
            let y2 = body_top + 50;
            let dur = (y2 - y) / filament_move_rate;
            let gear_dr = dur * gear_spin_rate;

            vid.add_key_frame('filament', t, { bottom: y });
            vid.add_key_frame('filament', t + dur, { bottom: y2 });

            vid.add_key_frame('gears', t, { lever_rotation: 100, gear_rotation: gear_r });
            vid.add_key_frame('gears', t + dur, { lever_rotation: 90, gear_rotation: gear_r + gear_dr });

            gear_r += gear_dr;
            t += dur;
        }

        // To the bottom
        {
            let y = body_top + 50;
            let y2 = heater_top;
            let dur = (y2 - y) / filament_move_rate;
            let gear_dr = dur * gear_spin_rate;

            vid.add_key_frame('filament', t, { bottom: y });
            vid.add_key_frame('filament', t + dur, { bottom: y2 });

            vid.add_key_frame('gears', t, { gear_rotation: gear_r });
            vid.add_key_frame('gears', t + dur, { gear_rotation: gear_r + gear_dr });

            gear_r += gear_dr;
            t += dur;
        }

        // Add the heater and nozzle
        {
            let y = heater_top;
            let y2 = bed_top;
            let dur = (y2 - y) / filament_move_rate;
            let gear_dr = dur * gear_spin_rate;

            vid.add_key_frame(['nozzle', 'heater', 'heater_labels'], t, { opacity: 0 });
            vid.add_key_frame(['nozzle', 'heater', 'heater_labels'], t + 0.5, { opacity: 1 });

            vid.add_key_frame('filament', t, { bottom: y });
            vid.add_key_frame('filament', t + dur, { bottom: y2 });

            vid.add_key_frame('gears', t, { gear_rotation: gear_r });
            vid.add_key_frame('gears', t + dur, { gear_rotation: gear_r + gear_dr });

            gear_r += gear_dr;
            t += dur;
        }

        // Pause
        t += 1;

        // Add fan
        {
            vid.add_key_frame('fan', t, { opacity: 0 });
            vid.add_key_frame('fan', t + 0.5, { opacity: 1 });
            t += 0.5;
        }

        filament_move_rate /= 4;
        gear_spin_rate /= 4;

        let flow_rate = filament_move_rate * (
            (filament_width * filament_width) / (nozzle_tip_width * nozzle_tip_width));

        // Draw layer 1
        {
            let x = 0;
            let x2 = 200;

            let dur = (x2 - x) / flow_rate
            let gear_dr = dur * gear_spin_rate;

            vid.add_key_frame('bed', t, { left: 0, layer1_length: 0 });
            vid.add_key_frame('bed', t + dur, { left: x2, layer1_length: -x2 });

            vid.add_key_frame('gears', t, { gear_rotation: gear_r });
            vid.add_key_frame('gears', t + dur, { gear_rotation: gear_r + gear_dr });

            t += dur;
            gear_r += gear_dr;
        }

        // Move z
        {
            vid.add_key_frame('bed', t, { top: 0 });
            vid.add_key_frame('bed', t + 0.5, { top: layer_height + layer_gap });
            t += 0.5;
        }

        // Layer 2
        {
            let x = 200;

            let dur = x / flow_rate
            let gear_dr = dur * gear_spin_rate;


            vid.add_key_frame('bed', t, { left: x, layer2_length: 0 });
            vid.add_key_frame('bed', t + dur, { left: 0, layer2_length: x });

            vid.add_key_frame('gears', t, { gear_rotation: gear_r });
            vid.add_key_frame('gears', t + dur, { gear_rotation: gear_r + gear_dr });

            t += dur;
            gear_r += gear_dr;
        }

        // Pause
        t += 1;

        {
            vid.add_transition('heatsink', t, 0.5, { opacity: 1 });
            t += 0.5;
        }

        // Pause
        t += 1;

        vid.set_duration(t);
    }

    if (mode == PRESENCE_MODE) {
        let t = 0;

        // Fade in initial stuff
        let bg_objs = ['title', 'heater', 'body', 'fan', 'heatsink', 'nozzle', 'bed', 'filament', 'center_channel', 'gears'];
        vid.add_key_frame(bg_objs, t + 0.5, { opacity: 1 });
        t += 0.5;

        // Sit for 1 second.
        t += 1;


        // Pixels/second
        let filament_move_rate = 100;

        // Rotations per second
        let gear_spin_rate = filament_move_rate / (2 * extruder_gear.outerRadius * Math.PI);
        // Radians per second
        gear_spin_rate = gear_spin_rate * (2 * Math.PI);


        {
            let y = body_top + 26;
            let dur = y / filament_move_rate;
            vid.add_key_frame('filament', t, { bottom: 0 });
            vid.add_key_frame('filament', t + dur, { bottom: y });
            t += dur;
        }

        // Pause
        t += 1;

        // Highlight area
        {
            vid.add_transition('hall_highlight', t, 0.5, { opacity: 1 });
            t += 0.5;
        }

        // Pause
        t += 1;

        // Show magnet
        {
            vid.add_transition('gears', t, 0.5, { magnet_opacity: 1 });
            t += 0.5;
        }

        // Pause
        t += 1;

        // Show sensor
        {
            vid.add_transition('hall_sensor', t, 0.5, { opacity: 1 });
            t += 0.5;
        }

        // Pause
        t += 1;

        // Highlight area
        {
            vid.add_transition('hall_highlight', t, 0.5, { opacity: 0 });
            t += 0.5;
        }

        let gear_r = 0;

        // Insert filament
        {
            let y = body_top + 26;
            let y2 = body_top + 50;
            let dur = (y2 - y) / filament_move_rate;
            let gear_dr = dur * gear_spin_rate;

            vid.add_key_frame('filament', t, { bottom: y });
            vid.add_key_frame('filament', t + dur, { bottom: y2 });

            vid.add_key_frame('gears', t, { lever_rotation: 100, gear_rotation: gear_r });
            vid.add_key_frame('gears', t + dur, { lever_rotation: 90, gear_rotation: gear_r + gear_dr });

            vid.add_key_frame('hall_sensor', t, { value: 1.5 });
            vid.add_key_frame('hall_sensor', t + dur, { value: 1.7 });

            gear_r += gear_dr;
            t += dur;
        }

        // To the bottom
        {
            let y = body_top + 50;
            let y2 = heater_top;
            let dur = (y2 - y) / filament_move_rate;
            let gear_dr = dur * gear_spin_rate;

            vid.add_key_frame('filament', t, { bottom: y });
            vid.add_key_frame('filament', t + dur, { bottom: y2 });

            vid.add_key_frame('gears', t, { gear_rotation: gear_r });
            vid.add_key_frame('gears', t + dur, { gear_rotation: gear_r + gear_dr });

            gear_r += gear_dr;
            t += dur;
        }

        // Pause
        t += 1;

        {
            vid.add_transition('hall_highlight2', t, 0.5, { opacity: 1 });
            t += 0.5;
        }

        // Pause
        t += 1;

        vid.set_duration(t);
    }

    if (mode == CONTROL_MODE) {
        let t = 0;

        vid.add_key_frame('gears', 0, { lever_rotation: 90 });
        vid.add_key_frame('filament', 0, { bottom: heater_bottom });

        // Fade in initial stuff
        let bg_objs = ['title', 'heater', 'body', 'fan', 'heatsink', 'nozzle', 'bed', 'filament', 'center_channel', 'gears', 'heater_labels'];
        vid.add_key_frame(bg_objs, t + 0.5, { opacity: 1 });
        t += 0.5;

        // Pause
        t += 1;

        let graph_obs = ['heater_temp_graph', 'heater_power_graph'];
        {
            vid.add_key_frame(graph_obs, t, { opacity: 0 });
            vid.add_key_frame(graph_obs, t + 0.5, { opacity: 1 });
            t += 0.5;
        }

        {
            vid.add_key_frame(graph_obs, t, { limit: 0 });
            vid.add_key_frame(graph_obs, t + 4, { limit: 1 });
            t += 4;
        }

        // Pause
        t += 1;

        {
            let objs = ['heater_highlight', 'heater_fade']

            vid.add_key_frame(objs, t, { opacity: 0 });
            vid.add_key_frame(objs, t + 0.5, { opacity: 1 });
            t += 0.5;
        }

        // Pause
        t += 1;


        {
            let objs = ['heater_highlight']

            vid.add_key_frame(objs, t, { opacity: 1 });
            vid.add_key_frame(objs, t + 0.5, { opacity: 0 });
            t += 0.5;
        }

        {
            let objs = ['heater_csense']

            vid.add_key_frame(objs, t, { opacity: 0 });
            vid.add_key_frame(objs, t + 0.5, { opacity: 1 });
            t += 0.5;
        }

        {
            let objs = ['heater_csense']

            vid.add_key_frame(objs, t, { t: 0 });
            vid.add_key_frame(objs, t + 10, { t: 1 });
            t += 10;
        }

        vid.set_duration(t);

    }


    return vid;
}
