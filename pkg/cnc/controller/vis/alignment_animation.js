import { Timeline, draw_title, deg2rad, draw_box, slide_body_grid, DiagramBox, WireBundle, Wire, shallow_copy, draw_multiline_text, draw_box_text } from './utils.js';
import { hexToRgba } from './hex_to_rgba.js';
import { drawArrow } from './arrow.js';
import { getPointAtY } from './y_point.js';
import { drawPolyline, drawSequentialChains, drawShearedSquare } from './sheared_square.js';
import { math_to_img, math_scale } from './mathjax.js';
import { drawCenteredTable } from './centered_table.js';
import { draw_graph } from './motion_animation.js';
import { getInterpolatedY, interpolateValue } from './linear_interp.js';

export async function configure(canvas) {
    // return part2_skew_video(canvas);
    // return part2_shrinkage_video(canvas);
    // return part5_triangulation_video(canvas);
    return await part5_math_video(canvas);
    // return await part5_cleaning_video(canvas);
    // return part6_video(canvas);
    // return part8_video(canvas);
    // return part8_video(canvas, true);
    // return part10_graph_video(canvas);
    // return part10_requests_video(canvas);
    // return part11_extrusion_video(canvas);
    // return part12_extrusion_video(canvas);
    // return part13_extrusion_video(canvas, false);
    // return part13_extrusion_video(canvas, true);
    // return part13_extrusion_video(canvas, true, true);
}

function part13_extrusion_video(canvas, fade, tilted = false) {
    let vid = new Timeline();

    let title = 'Applying the Mesh';
    vid.set_name("part13_extrusion");

    if (fade) {
        title = 'Z Fading'
        vid.set_name('part13_zfade');
    }
    if (tilted) {
        title = 'Z Fading on a Tiled Bed';
        vid.set_name('part13_zfade_tilted');
    }


    vid.add_object('title', { opacity: 0, text: title }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    let bed_width = 700;
    let bed_height = 40;
    let sheet_height = 4;
    let layer_height = 6;
    let layer_gap = 2;

    let bed_offsets = [];
    let num_chunks = 10;
    for (var i = 0; i < num_chunks; i++) {
        let x = (-bed_width / 2) + i * (bed_width / num_chunks);
        let y = -(sheet_height / 2) - (i % 2 == 0 ? 0 : 10);
        bed_offsets.push({ x, y });
    }


    vid.add_object('bed', { opacity: 0 }, (ctx, params) => {

        ctx.translate(centerX, 350);

        if (tilted) {
            ctx.rotate(deg2rad(10));
        }


        ctx.fillStyle = '#888';
        ctx.translate(0, (sheet_height / 2) + (layer_height / 2));
        draw_box(ctx, bed_width, sheet_height);

        {
            ctx.beginPath();
            ctx.moveTo(-bed_width / 2, 0);
            ctx.lineTo(-bed_width / 2, sheet_height / 2);

            bed_offsets.map((p) => {
                ctx.lineTo(p.x, p.y);
            })

            ctx.lineTo(bed_width / 2, sheet_height / 2);
            ctx.moveTo(-bed_width / 2, 0);
            ctx.closePath();
            ctx.fill();
        }


        ctx.fillStyle = '#ddd';
        ctx.font = '20px "Noto Sans"';
        ctx.translate(0, (sheet_height / 2) + (bed_height / 2));
        draw_box_text(ctx, bed_width, bed_height, 'Bed');
    });

    let top = -6 + (layer_height / 2);

    let strands = [];
    let num_layers = 20;

    for (var i = 0; i < num_layers; i++) {

        let start_x = - 200 - 2.5;
        let end_x = + 200 + 2.5;

        let num_points = 100;

        let strand = [];
        for (var j = 0; j < (num_points + 1); j++) {
            let x = start_x + (j * ((end_x - start_x) / num_points));

            let zero_offset = getInterpolatedY(bed_offsets, x);

            if (tilted) {
                zero_offset += Math.sin(deg2rad(10)) * x;
            }

            if (fade) {
                if (i >= 10) {
                    zero_offset = 0;
                } else {
                    zero_offset *= (10 - i) / 10;
                }
            }

            let y = zero_offset - (layer_height + layer_gap) * i;
            strand.push({ x, y });
        }

        if (i % 2 == 0) {
            strand.reverse();
        }

        strands.push(strand);
    }

    vid.add_object('filament', { opacity: 1, progress: 0, lift: 0 }, (ctx, params) => {
        ctx.translate(centerX, 350);

        ctx.lineWidth = layer_height;
        ctx.strokeStyle = '#0bf';
        drawSequentialChains(ctx, strands, params.progress);
    })


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'bed'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['filament'], t, 4, { progress: 1 });
    t += 4;
    t += pause;


    vid.set_duration(t);

    return vid;
}

function part12_extrusion_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part12_extrusion");

    vid.add_object('title', { opacity: 0, text: 'Slanted Printing' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    let bed_width = 700;
    let bed_height = 40;
    let sheet_height = 4;
    let layer_height = 6;
    let layer_gap = 2;


    vid.add_object('bed', { opacity: 0 }, (ctx, params) => {

        ctx.translate(centerX, 350);
        ctx.rotate(deg2rad(20));

        ctx.fillStyle = '#888';
        ctx.translate(0, (sheet_height / 2) + (layer_height / 2));
        draw_box(ctx, bed_width, sheet_height);

        ctx.fillStyle = '#ddd';
        ctx.font = '20px "Noto Sans"';
        ctx.translate(0, (sheet_height / 2) + (bed_height / 2));
        draw_box_text(ctx, bed_width, bed_height, 'Bed');
    });

    let top = -6 + (layer_height / 2);

    let strands = [];

    for (var i = 0; i < 10; i++) {
        let strand = [
            { x: - 200 - 2.5, y: top - (layer_height + layer_gap) * i },
            { x: + 200 + 2.5, y: top - (layer_height + layer_gap) * i },
        ];

        if (i % 2 == 0) {
            strand.reverse();
        }

        strands.push(strand);
    }



    vid.add_object('filament', { opacity: 1, progress: 0, lift: 0 }, (ctx, params) => {
        ctx.translate(centerX, 350);
        ctx.rotate(deg2rad(20));

        ctx.lineWidth = layer_height;
        ctx.strokeStyle = '#0bf';
        drawSequentialChains(ctx, strands, params.progress);
    })


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'bed'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['filament'], t, 4, { progress: 1 });
    t += 4;
    t += pause;


    vid.set_duration(t);

    return vid;
}

function part11_extrusion_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part11_extrusion");

    vid.add_object('title', { opacity: 0, text: 'Extruder Synchronization' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    let bed_width = 700;
    let bed_height = 40;
    let sheet_height = 4;
    let layer_height = 6;
    let layer_gap = 2;


    vid.add_object('bed', { opacity: 0 }, (ctx, params) => {

        ctx.translate(centerX, 350);

        ctx.fillStyle = '#888';
        ctx.translate(0, (sheet_height / 2) + (layer_height / 2));
        draw_box(ctx, bed_width, sheet_height);

        ctx.fillStyle = '#ddd';
        ctx.font = '20px "Noto Sans"';
        ctx.translate(0, (sheet_height / 2) + (bed_height / 2));
        draw_box_text(ctx, bed_width, bed_height, 'Bed');
    });

    let top = 346 + (layer_height / 2);

    let strands = [];

    for (var i = 0; i < 10; i++) {
        let strand = [
            { x: centerX - 200 - 2.5, y: top - (layer_height + layer_gap) * i },
            { x: centerX + 200 + 2.5, y: top - (layer_height + layer_gap) * i },
        ];

        let shift = (Math.random() - 0.5) * 50;

        strand.map((p) => {
            p.x += shift;
        });

        if (i % 2 == 0) {
            strand.reverse();
        }

        strands.push(strand);
    }



    vid.add_object('filament', { opacity: 1, progress: 0, lift: 0 }, (ctx, params) => {
        // 346 is the nozzle_bottom
        let y = 346 + (layer_height / 2) - params.lift;

        ctx.lineWidth = layer_height;
        ctx.strokeStyle = '#0bf';
        drawSequentialChains(ctx, strands, params.progress);
    })


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'bed'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['filament'], t, 4, { progress: 1 });
    t += 4;
    t += pause;


    vid.set_duration(t);

    return vid;
}

function part10_requests_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part10_requests");

    vid.add_object('title', { opacity: 0, text: 'Time Synchronization' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });


    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(1, 3);

    let pi_cell = box_grid.cell(0, 0);;
    let pi_box = new DiagramBox({
        text: 'Raspberry Pi',
        width: pi_cell.width() - 40,
        height: pi_cell.height() - 20,
        // font_size: 20,
        text_offset: { x: 0, y: -150 },
        position: {
            x: pi_cell.center().x - 20,
            y: pi_cell.center().y
        }
    });

    let main_cell = box_grid.cell(0, 2).split(3, 1).cell(2, 0);
    let main_box = new DiagramBox({
        text: 'Main\nMCU',
        width: main_cell.width() - 20,
        height: main_cell.height() - 20,
        font_size: 20,
        text_offset: { x: -50, y: 0 },
        position: main_cell.center()
    });

    let tool_cell = box_grid.cell(0, 2).split(3, 1).cell(0, 0);
    let tool_box = new DiagramBox({
        text: 'Toolhead\nMCU',
        width: tool_cell.width() - 20,
        height: tool_cell.height() - 20,
        font_size: 20,
        text_offset: { x: -50, y: 0 },
        position: tool_cell.center()
    });


    vid.add_object('pi', { opacity: 0, lines: [] }, (ctx, params) => {
        pi_box.draw(ctx);

        let lines = params.lines;

        for (var i = 0; i < lines.length; i++) {
            let position = shallow_copy(pi_box.position());
            position.y -= 0;

            position.y += 60 * i;

            let b = new DiagramBox({
                text: lines[i],
                width: 200,
                height: 40,
                position,
                font_size: 16,
                text_color: '#000',
                font_family: "Noto Sans Mono",
                background_color: '#fff',
            });
            b.draw(ctx);
        }
    });

    vid.add_object('main', { opacity: 0 }, (ctx, params) => {
        main_box.draw(ctx);
    });

    vid.add_object('tool', { opacity: 0 }, (ctx, params) => {
        tool_box.draw(ctx);
    });


    function draw_clock(ctx, position, v) {
        let b = new DiagramBox({
            text: `T=${Math.round(v)}`,
            width: 80,
            height: 50,
            position,
            font_size: 16,
            text_color: '#fff',
            font_family: "Noto Sans Mono",
            background_color: '#666',
        });
        b.draw(ctx);
    }

    vid.add_object('clocks', { opacity: 0, t: 0 }, (ctx, params) => {

        {
            let p = shallow_copy(pi_box.position());
            p.y -= 90;
            draw_clock(ctx, p, params.t + 1);
        }

        {
            let p = shallow_copy(main_box.position());
            p.x += 50;
            draw_clock(ctx, p, params.t + 10);
        }

        {
            let p = shallow_copy(tool_box.position());
            p.x += 50;
            draw_clock(ctx, p, params.t + 24);
        }

    });

    vid.add_object('mcu_arrow_forward', { opacity: 0, t: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';

        let x_start = pi_box.right_center().x;
        let x_end = main_box.left_center().x;

        let end = x_start + (params.t * (x_end - x_start));
        if (end < x_start + 20) {
            end = x_start + 20;
        }

        drawArrow(
            ctx,
            x_start, main_box.left_center().y - 20,
            end, main_box.left_center().y - 20,
            2, 20, false
        );
    });

    vid.add_object('tool_arrow_forward', { opacity: 0, t: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';

        let x_start = pi_box.right_center().x;
        let x_end = tool_box.left_center().x;

        let end = x_start + (params.t * (x_end - x_start));
        if (end < x_start + 20) {
            end = x_start + 20;
        }

        drawArrow(
            ctx,
            x_start, tool_box.left_center().y - 20,
            end, tool_box.left_center().y - 20,
            2, 20, false
        );
    });

    vid.add_object('mcu_arrow_back', { opacity: 0, t: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';

        let x_start = main_box.left_center().x;
        let x_end = pi_box.right_center().x;

        let end = x_start + (params.t * (x_end - x_start));
        if (end > x_start - 20) {
            end = x_start - 20;
        }

        drawArrow(
            ctx,
            x_start, main_box.left_center().y + 20,
            end, main_box.left_center().y + 20,
            2, 20, false
        );
    });


    vid.add_object('sof', { opacity: 0 }, (ctx, params) => {
        ctx.translate(
            pi_box.right_center().x + 10,
            pi_box.top_center().y + 20
        );
        draw_multiline_text(ctx, {
            text: `USB SOF Packets`,
            font_size: 20,
            font_family: "Noto Sans Mono",
            text_align: 'left',
            color: '#000'
        });
    })


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'clocks', 'pi', 'main', 'tool'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_key_frame('pi', t, { lines: ['SendTime = 1'] });
    t += pause;

    // vid.add_key_frame('pi', t, { lines: ['SendTime = 1'] });
    // t += pause;

    vid.add_transition('mcu_arrow_forward', t, 1, { opacity: 1, t: 1 });
    vid.add_transition('clocks', t, 1, { t: 10 });
    t += 1;
    t += pause;

    vid.add_transition('mcu_arrow_back', t, 1, { opacity: 1, t: 1 });
    vid.add_transition('clocks', t, 1, { t: 20 });
    t += 1;

    vid.add_key_frame('pi', t, { lines: ['SendTime = 1', 'ReceivedTime = 21', 'RemoteTime = 20'] });
    t += pause;

    // Resetting
    vid.add_key_frame('pi', t, { lines: [] });
    vid.add_transition('mcu_arrow_back', t, 0.1, { opacity: 0, t: 0 });
    vid.add_transition('mcu_arrow_forward', t, 0.1, { opacity: 0, t: 0 });
    t += pause;

    vid.add_transition('sof', t, 0.5, { opacity: 1 });
    t == 0.5;
    t += pause;

    let time_offset = 20;

    for (var i = 0; i < 20; i++) {
        vid.add_transition('mcu_arrow_forward', t, 0.5, { opacity: 1, t: 1 });
        vid.add_transition('tool_arrow_forward', t, 0.5, { opacity: 1, t: 1 });
        vid.add_transition('clocks', t, 0.5, { t: time_offset + 10 });
        t += 0.5;
        time_offset += 10;

        vid.add_key_frame('pi', t, {
            lines: [
                `MCU1_SOF_TIME = ${time_offset + 10}`,
                `MCU2_SOF_TIME = ${time_offset + 24}`
            ]
        });

        t += 0.5;


        vid.add_transition('clocks', t, 0.5, { t: time_offset + 10 });
        time_offset += 10;
        vid.add_transition('mcu_arrow_forward', t, 0.1, { opacity: 0 });
        vid.add_transition('tool_arrow_forward', t, 0.1, { opacity: 0 });
        t += 0.1;
        vid.add_transition('mcu_arrow_forward', t, 0.1, { t: 0 });
        vid.add_transition('tool_arrow_forward', t, 0.1, { t: 0 });
        t += 0.1;

        t += 0.3;


    }



    // mcu_arrow_back


    vid.set_duration(t);

    return vid;
}

function part10_graph_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part10_graph");

    vid.add_object('title', { opacity: 0, text: 'Time Synchronization' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let grid = slide_body_grid(canvas).split(1, 1);
    let cell1 = grid.cell(0, 0);

    function get_xy(xt, yt) {
        let x = xt * cell1.width();
        let y = -yt * (cell1.height() - 20)

        x += 3;
        y -= 3;

        return {
            x, y
        }
    }

    let coeff2 = 0.6;
    let coeff3 = 1.1;
    vid.add_object('coeffs', { opacity: 1, coeff2: coeff2, coeff3: coeff3 }, (ctx, params) => {
        coeff2 = params.coeff2;
        coeff3 = params.coeff3;
    })

    vid.add_object('graph1', {
        opacity: 0,
        max1: 0,
        max2: 0,
        max3: 0,
    }, (ctx, params) => {

        let pos1 = cell1.bottom_left();
        ctx.save();
        ctx.translate(pos1.x, pos1.y);

        let ft = (t) => {
            if (t > params.max1) {
                return;
            }

            let y = t;

            return {
                x: t,
                y
            };
        };

        let ft2 = (t) => {
            if (t > params.max2) {
                return;
            }

            return {
                x: t,
                y: coeff2 * t + 0.1
            }
        }

        let ft3 = (t) => {
            if (t > params.max3) {
                return;
            }

            let y = coeff3 * t + 0.05;
            if (y > 1.0) {
                return;
            }

            return {
                x: t,
                y
            }
        }

        ctx.save();
        draw_graph(ctx, {
            width: cell1.width(),
            height: cell1.height() - 20,
            y_label: 'Clock Value',
            x_label: 'Real Time',
            series: [
                {
                    color: '#0bf',
                    f: ft
                },
                {
                    color: '#f00',
                    f: ft2
                },
                {
                    color: '#081',
                    f: ft3
                }
            ],
            font_size: 20
        }, ft);
        ctx.restore();

        ctx.restore();

        {
            ctx.save();
            ctx.translate(
                850,
                95
            );
            draw_multiline_text(ctx, {
                text: `Pi Clock`,
                font_size: 20,
                font_family: "Noto Sans Mono",
                text_align: 'left',
                color: hexToRgba('#0bf', params.max1)
            });
            ctx.restore();
        }
        {
            ctx.save();
            ctx.translate(
                850,
                210
            );
            draw_multiline_text(ctx, {
                text: `Main MCU`,
                font_size: 20,
                font_family: "Noto Sans Mono",
                text_align: 'left',
                color: hexToRgba('#f00', params.max2)
            });
            ctx.restore();
        }
        {
            ctx.save();
            ctx.translate(
                650,
                110
            );
            draw_multiline_text(ctx, {
                text: `Toolhead`,
                font_size: 20,
                font_family: "Noto Sans Mono",
                text_align: 'left',
                color: hexToRgba('#081', params.max3)
            });
            ctx.restore();
        }
    });

    vid.add_object('vert_line', { opacity: 0 }, (ctx, params) => {

        let base = cell1.bottom_left();

        let p1 = get_xy(0.5, 0.5);
        let p2 = get_xy(0.5, coeff2 * 0.5 + 0.1);
        let p3 = get_xy(0.5, coeff3 * 0.5 + 0.05);


        ctx.save();
        ctx.beginPath();
        ctx.moveTo(base.x + p1.x, cell1.top_center().y);
        ctx.lineTo(base.x + p1.x, cell1.bottom_center().y);
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;
        ctx.setLineDash([5, 5]);
        ctx.stroke();
        ctx.restore();

        {
            let radius = 6;
            ctx.beginPath();
            ctx.arc(base.x + p1.x, base.y + p1.y, radius, 0, Math.PI * 2); // Draw a full circle
            ctx.fillStyle = hexToRgba('#0bf', 1.0);
            ctx.fill();
            ctx.closePath();
        }
        {
            let radius = 6;
            ctx.beginPath();
            ctx.arc(base.x + p2.x, base.y + p2.y, radius, 0, Math.PI * 2); // Draw a full circle
            ctx.fillStyle = hexToRgba('#f00', 1.0);
            ctx.fill();
            ctx.closePath();
        }
        {
            let radius = 6;
            ctx.beginPath();
            ctx.arc(base.x + p3.x, base.y + p3.y, radius, 0, Math.PI * 2); // Draw a full circle
            ctx.fillStyle = hexToRgba('#081', 1.0);
            ctx.fill();
            ctx.closePath();
        }
    });

    let random_nums = [];
    for (var i = 0; i < 20; i++) {
        random_nums.push(Math.random());
    }

    vid.add_object('samples', { opacity: 1, t: -0.01, jitter: 0 }, (ctx, params) => {
        let base = cell1.bottom_left();

        for (var i = 0; i < (10 + 1); i++) {

            let x = i * 0.1;
            if (x > params.t) {
                break;
            }

            let y = coeff2 * x + 0.1;
            y += params.jitter * (random_nums[i] - 0.5);

            let pt = get_xy(x, y);
            {
                let radius = 6;
                ctx.beginPath();
                ctx.arc(base.x + pt.x, base.y + pt.y, radius, 0, Math.PI * 2); // Draw a full circle
                ctx.fillStyle = hexToRgba('#f00', 1.0);
                ctx.fill();
                ctx.closePath();
            }
        }
    });



    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'graph1'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('graph1', t, 0.5, { max1: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('graph1', t, 0.5, { max2: 1, max3: 1, });
    t += 0.5;
    t += pause;


    vid.add_transition('vert_line', t, 0.5, { opacity: 1, });
    t += 0.5;
    t += pause;

    vid.add_transition('coeffs', t, 0.5, { coeff2: 0.4, coeff3: 1.3 });
    t += 0.5;
    vid.add_transition('coeffs', t, 0.5, { coeff2: 0.6, coeff3: 1.1 });
    t += 0.5;
    t += pause;

    // Reset for showing linear interpolation.
    vid.add_transition('vert_line', t, 0.5, { opacity: 0, });
    vid.add_transition('graph1', t, 0.5, { max2: 0, max3: 0, });
    t += 0.5;
    t += pause;

    vid.add_transition('samples', t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('samples', t, 0.5, { jitter: 0.1 });
    t += 0.5;
    t += pause;

    vid.add_transition('graph1', t, 0.5, { max2: 1 });
    t += 0.5;
    t += pause;


    vid.set_duration(t);

    return vid;

}

function part8_video(canvas, part9) {
    let vid = new Timeline();

    if (part9) {
        vid.set_name('part9');
    } else {
        vid.set_name('part8');
    }

    vid.add_object('title', { opacity: 0, text: part9 ? 'Timing Correction' : 'Life of a Tap' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(1, 4);

    let pi_cell = box_grid.cell(0, 0);;
    let pi_box = new DiagramBox({
        text: 'Raspberry Pi',
        width: pi_cell.width() - 40,
        height: pi_cell.height() - 20,
        // font_size: 20,
        // text_offset: { x: 0, y: -105 },
        position: {
            x: pi_cell.center().x - 20,
            y: pi_cell.center().y
        }
    });

    let main_cell = box_grid.cell(0, 1.5).split(4, 1).cell(3, 0);
    let main_box = new DiagramBox({
        text: 'Main\nMCU',
        width: main_cell.width() - 20,
        height: main_cell.height() - 20,
        font_size: 20,
        // text_offset: { x: 0, y: -105 },
        position: main_cell.center()
    });

    let motor_cell = box_grid.cell(0, 3).split(4, 1).cell(3, 0);
    let motor_box = new DiagramBox({
        text: 'Z Motor\n(Position = -5mm)',
        width: motor_cell.width() - 20,
        height: motor_cell.height() - 20,
        font_size: 20,
        // text_offset: { x: 0, y: -105 },
        position: {
            x: motor_cell.center().x + 10,
            y: motor_cell.center().y
        }
    });

    let nozzle_cell = box_grid.cell(0, 3).split(4, 1).cell(0, 0);
    let nozzle_box = new DiagramBox({
        text: 'Nozzle\nSensor',
        width: nozzle_cell.width() - 20,
        height: nozzle_cell.height() - 20,
        font_size: 20,
        // text_offset: { x: 0, y: -105 },
        position: {
            x: nozzle_cell.center().x + 10,
            y: nozzle_cell.center().y
        }
    });

    let tool_cell = box_grid.cell(0, 1.5).split(4, 1).cell(0, 0);
    let tool_box = new DiagramBox({
        text: 'Toolhead\nMCU',
        width: tool_cell.width() - 20,
        height: tool_cell.height() - 20,
        font_size: 20,
        // text_offset: { x: 0, y: -105 },
        position: tool_cell.center()
    });


    vid.add_object('pi', { opacity: 0 }, (ctx, params) => {
        pi_box.draw(ctx);
    });

    vid.add_object('main', { opacity: 0, text_x: 0 }, (ctx, params) => {
        main_box._text_offset = { x: params.text_x, y: 0 };
        main_box.draw(ctx);
    });

    vid.add_object('main_start_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            pi_box.right_center().x, main_box.left_center().y,
            main_box.left_center().x, main_box.left_center().y,
            2, 20, false
        );

        ctx.translate(
            pi_box.right_center().x + 5,
            main_box.left_center().y - 15
        );
        draw_multiline_text(ctx, {
            text: `"Move Up"`,
            font_size: 20,
            text_align: 'left',
            color: '#000'
        })
    });


    vid.add_object('main_start_arrow2', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            pi_box.right_center().x, main_box.left_center().y,
            main_box.left_center().x, main_box.left_center().y,
            2, 20, false
        );

        ctx.translate(
            pi_box.right_center().x + 5,
            main_box.left_center().y - 40
        );
        draw_multiline_text(ctx, {
            text: `Move 5mm\nStart Time=100\nEnd Time=200`,
            font_size: 18,
            text_align: 'left',
            color: '#000'
        })
    });


    vid.add_object('main_stop_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            pi_box.right_center().x, main_box.left_center().y,
            main_box.left_center().x, main_box.left_center().y,
            2, 20, false
        );

        ctx.translate(
            pi_box.right_center().x + 5,
            main_box.left_center().y - 30
        );
        draw_multiline_text(ctx, {
            text: `"Stop"\n(150us ± 50us)`,
            font_size: 18,
            text_align: 'left',
            color: '#000'
        })
    });




    vid.add_object('motor', { opacity: 0, pos: -5 }, (ctx, params) => {

        let pos = Math.round(params.pos * 1000) / 1000;
        motor_box.set_text(`Motor\n(Z = ${pos}mm)`);

        motor_box.draw(ctx);


        // Main -> motor arrow
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            main_box.right_center().x, main_box.right_center().y,
            motor_box.left_center().x, motor_box.left_center().y,
            2, 20, false
        );
    });

    vid.add_object('motor_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.translate(
            main_box.right_center().x + 5,
            main_box.right_center().y - 30
        );
        draw_multiline_text(ctx, {
            text: `"Finish Step"\n(0 - 250us)`,
            font_size: 18,
            text_align: 'left',
            color: '#000'
        });
    });

    vid.add_object('bed', { opacity: 0, offset: 0, }, (ctx, params) => {

        let cell = box_grid.cell(0, 3).split(4, 1).cell(2, 0);
        let bed_height = 30;
        let bed_width = cell.width();
        let sheet_height = 4;
        let layer_height = 6;

        // Motor shaft
        {
            let shaft_width = 20;

            ctx.save();

            let shaft_y_top = cell.center().y + sheet_height + bed_height - params.offset;

            ctx.fillStyle = '#444';
            ctx.fillRect(
                motor_box.top_center().x - (shaft_width / 2),
                shaft_y_top,
                shaft_width,
                motor_box.top_center().y - shaft_y_top
            );

            ctx.restore();
        }

        ctx.translate(motor_box.top_center().x, cell.center().y - params.offset);

        ctx.fillStyle = '#888';
        ctx.translate(0, (sheet_height / 2) + (layer_height / 2));
        draw_box(ctx, bed_width, sheet_height);

        ctx.fillStyle = '#ddd';
        ctx.font = '20px "Noto Sans"';
        ctx.translate(0, (sheet_height / 2) + (bed_height / 2));
        draw_box_text(ctx, bed_width, bed_height, 'Bed');
    });

    vid.add_object('nozzle', { opacity: 0 }, (ctx, params) => {
        let cell = nozzle_cell;
        nozzle_box.draw(ctx);

        ctx.translate(nozzle_box.bottom_center().x, nozzle_box.bottom_center().y);
        draw_nozzle(ctx);
    });

    vid.add_object('toolhead', { opacity: 0, text_x: 0 }, (ctx, params) => {
        tool_box._text_offset = { x: params.text_x, y: 0 };
        tool_box.draw(ctx);

    })

    vid.add_object('adc_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            nozzle_box.left_center().x, nozzle_box.left_center().y,
            tool_box.right_center().x, tool_box.right_center().y,
            2, 20, false
        );

        ctx.translate(
            nozzle_box.left_center().x - 5,
            nozzle_box.left_center().y - (part9 ? 30 : 20)
        );
        draw_multiline_text(ctx, {
            text: part9 ? 'Event\n(Time = 123)' : `ADC (1-120us)`,
            font_size: 18,
            text_align: 'right',
            color: '#000'
        })
    });


    vid.add_object('adc_arrow2', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            nozzle_box.left_center().x, nozzle_box.left_center().y,
            main_box.top_center().x, main_box.top_center().y,
            2, 20, false
        );
    });


    vid.add_object('tool_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            tool_box.left_center().x, tool_box.left_center().y,
            pi_box.right_center().x, tool_box.right_center().y,
            2, 20, false
        );

        ctx.translate(
            tool_box.left_center().x - 5,
            tool_box.left_center().y - 30
        );
        draw_multiline_text(ctx, {
            text: part9 ? `"Hit"\n(Time = 123)` : `"Hit"\n(150us ± 50us)`,
            font_size: 18,
            text_align: 'right',
            color: '#000'
        })
    });


    vid.add_object('motor_highlight', { opacity: 0 }, (ctx, params) => {

        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 4;

        let w = motor_box._width + 30;
        let h = motor_box._height + 30;

        ctx.strokeRect(
            motor_box.position().x - w / 2,
            motor_box.position().y - h / 2,
            w, h
        );
    });

    vid.add_object('history', { opacity: 0, t: 0 }, (ctx, params) => {
        ctx.translate(pi_box.position().x, pi_box.position().y);
        const headers = ["Time", "Z"];
        const rawValues = ["122", "-0.1", "123", "0", "124", "0.1", "125", "0.2"];
        drawCenteredTable(ctx, 0, 0, 150, 120, headers, rawValues, 1, params.t);
    })

    vid.add_object('history_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';

        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(pi_box.right_center().x, tool_box.left_center().y);
        ctx.lineTo(pi_box.position().x, tool_box.left_center().y);
        ctx.stroke();

        drawArrow(
            ctx,
            pi_box.position().x, tool_box.left_center().y,
            pi_box.position().x, pi_box.position().y - 70,
            2, 20, false
        );
    });

    function draw_clock(ctx, position, v) {
        let b = new DiagramBox({
            text: `T=${Math.round(v)}`,
            width: 80,
            height: 50,
            position,
            font_size: 16,
            text_color: '#fff',
            font_family: "Noto Sans Mono",
            background_color: '#666',
        });
        b.draw(ctx);
    }

    vid.add_object('clocks', { opacity: 0, t: 0 }, (ctx, params) => {

        {
            let p = shallow_copy(pi_box.position());
            p.y += 60;
            draw_clock(ctx, p, params.t + 1);
        }

        {
            let p = shallow_copy(main_box.position());
            p.x += 50;
            draw_clock(ctx, p, params.t + 10);
        }

        {
            let p = shallow_copy(tool_box.position());
            p.x += 50;
            draw_clock(ctx, p, params.t + 24);
        }

    });


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'pi'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['main', 'main_start_arrow', 'motor', 'bed'], t, 0.5, { opacity: 1 });
    t += 0.5;

    /*
    100 is pre-hit
    130 is hit
    */

    function offset_to_pos(offset) {
        return -5 + (offset / 130) * 5
    }

    vid.add_transition('bed', t, 0.5, { offset: 100 });
    vid.add_transition('motor', t, 0.5, { pos: offset_to_pos(100) });
    t += 0.5;
    vid.add_transition('main_start_arrow', t, 0.5, { opacity: 0 });
    t += 0.5;

    if (part9) {
        vid.set_start_time(t);
    }

    t += pause;

    if (part9) {
        vid.add_transition(['main_start_arrow2'], t, 0.5, { opacity: 1 });
        t += 0.5;
        t += pause;
    }

    vid.add_transition(['nozzle'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('bed', t, 0.5, { offset: 130 });
    vid.add_transition('motor', t, 0.5, { pos: offset_to_pos(130) });
    t += 0.5;
    t += pause;

    vid.add_transition(['adc_arrow', 'toolhead'], t, 0.5, { opacity: 1 });
    vid.add_transition('bed', t, 0.5, { offset: 133 });
    vid.add_transition('motor', t, 0.5, { pos: 0.002 });
    t += 0.5;
    t += pause;

    vid.add_transition(['tool_arrow'], t, 0.5, { opacity: 1 });
    vid.add_transition('bed', t, 0.5, { offset: 136 });
    vid.add_transition('motor', t, 0.5, { pos: 0.004 });
    t += 0.5;
    t += pause;


    if (part9) {
        vid.add_transition(['history'], t, 0.5, { opacity: 1 });
        t += 0.5;
        t += pause;


        vid.add_transition(['history'], t, 0.5, { t: 1 });
        vid.add_transition(['history_arrow'], t, 0.5, { opacity: 1 });
        t += 0.5;
        t += pause;


        vid.add_transition(['history_arrow', 'history', 'tool_arrow', 'adc_arrow', 'main_start_arrow2'], t, 0.5, { opacity: 0 });
        t += 0.5;
        t += pause;

        vid.add_transition('clocks', t, 0.5, { opacity: 1 });
        vid.add_transition(['main', 'toolhead'], t, 0.5, { text_x: -50 });
        t += 0.5;
        t += pause;


        vid.add_transition('clocks', t, 4, { t: 30 });
        t += 4;
        t += pause;


    } else {
        vid.add_transition(['main_stop_arrow'], t, 0.5, { opacity: 1 });
        vid.add_transition('bed', t, 0.5, { offset: 140 });
        vid.add_transition('motor', t, 0.5, { pos: 0.005 });
        t += 0.5;
        t += pause;

        vid.add_transition(['motor_arrow'], t, 0.5, { opacity: 1 });
        t += 0.5;
        t += pause;

        vid.add_transition(['motor_highlight'], t, 0.5, { opacity: 1 });
        t += 0.5;
        t += pause;

        vid.add_transition(['adc_arrow2'], t, 0.5, { opacity: 1 });
        vid.add_transition(['adc_arrow', 'motor_highlight', 'main_stop_arrow', 'tool_arrow', 'toolhead'], t, 0.5, { opacity: 0 });
        vid.add_transition('motor', t, 0.5, { pos: 0.001 });
        t += 0.5;
        t += pause;
    }






    vid.set_duration(t);

    return vid;
}

function draw_nozzle(ctx) {

    let channel_width = 20;

    let nozzle_upper_width = 100;
    let nozzle_lower_width = 40;
    let nozzle_tip_width = channel_width / 4;

    let nozzle_upper_height = 10;
    let nozzle_lower_height = 30;

    let nozzle_top = 0;
    let nozzle_bottom = nozzle_top + nozzle_upper_height + nozzle_lower_height;

    let filament_color = '#0af';


    ctx.save();
    ctx.translate(0, (nozzle_bottom - nozzle_top) / 2);
    ctx.fillStyle = filament_color;
    ctx.strokeStyle = filament_color;
    draw_box(ctx, channel_width, (nozzle_bottom - nozzle_top));
    ctx.restore();

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

}

function part6_video(canvas) {

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    let vid = new Timeline();
    vid.set_name('part6');

    vid.add_object('title', { opacity: 0, text: 'Bed Alignment' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let bed_width = 700;
    let bed_height = 40;
    let sheet_height = 4;
    let layer_height = 6;

    let random_numbers = [];
    for (var i = 0; i < 100; i++) {
        let v = Math.random();
        v = Math.round(v * 5) / 5;
        random_numbers.push(v);
    }

    vid.add_object('bed', { opacity: 0, lift: 0, rotation: 0 }, (ctx, params) => {

        ctx.translate(centerX, 350 + Math.sin(params.rotation) * 100);
        ctx.rotate(params.rotation);

        ctx.fillStyle = '#888';
        ctx.translate(0, (sheet_height / 2) + (layer_height / 2));
        draw_box(ctx, bed_width, sheet_height);

        {
            ctx.beginPath();
            ctx.moveTo(-bed_width / 2, 0);
            ctx.lineTo(-bed_width / 2, sheet_height / 2);

            let num_chunks = 30;

            for (var i = 0; i < num_chunks; i++) {

                let x = (-bed_width / 2) + i * (bed_width / num_chunks);
                let y = -(sheet_height / 2) - params.lift * random_numbers[i];

                ctx.lineTo(x, y);
            }


            ctx.lineTo(bed_width / 2, sheet_height / 2);
            ctx.moveTo(-bed_width / 2, 0);
            ctx.closePath();
            ctx.fill();
        }

        ctx.fillStyle = '#ddd';
        ctx.font = '20px "Noto Sans"';
        ctx.translate(0, (sheet_height / 2) + (bed_height / 2));
        draw_box_text(ctx, bed_width, bed_height, 'Bed');
    });

    vid.add_object('toolhead', { opacity: 0, x: centerX - 200 }, (ctx, params) => {

        let pos = { x: params.x, y: 222 - layer_height };
        let toolhead_height = 180;

        let toolhead = new DiagramBox({
            text: 'Toolhead',
            width: 180,
            height: toolhead_height,
            font_size: 20,
            text_offset: { x: 0, y: -105 },
            position: pos
        });


        // Nozzle stuff


        let nozzle_top = pos.y + toolhead_height / 2;


        {
            ctx.save();
            ctx.translate(params.x, nozzle_top);

            draw_nozzle(ctx);

            ctx.restore();
        }


        toolhead.draw(ctx);

    })


    vid.add_object('filament', { opacity: 1, progress: 0, lift: 0 }, (ctx, params) => {
        // 346 is the nozzle_bottom
        let y = 346 + (layer_height / 2) - params.lift;

        ctx.lineWidth = layer_height;
        ctx.strokeStyle = '#0bf';
        drawPolyline(ctx, [
            { x: centerX - 200 - 2.5, y },
            { x: centerX + 200 + 2.5, y },
        ], 0.5 * params.progress);
    })

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'bed'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;

    vid.add_transition(['toolhead'], t, 0.5, { opacity: 1 })
    t += 0.5;


    vid.add_transition('filament', t, 1, { progress: 1 });
    vid.add_transition('toolhead', t, 1, { x: centerX + 200 });
    t += 1;
    t += pause;

    vid.add_transition(['toolhead'], t, 0.5, { opacity: 0 })
    t += 0.5;
    t += pause;

    vid.add_transition(['filament', 'bed'], t, 0.5, { lift: 30 })
    t += 0.5;
    t += pause;

    vid.add_transition(['bed'], t, 0.5, { rotation: deg2rad(5) })
    t += 0.5;
    t += pause;

    // Show the bed with deflections.


    vid.set_duration(t);

    return vid;
}

async function part5_cleaning_video(canvas) {
    let vid = new Timeline();
    vid.set_name('part5_cleaning');

    vid.add_object('title', { opacity: 0, text: 'Skew Normalization' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 1);


    let two_grid = body_grid.split(2, 1);


    let top_cell = box_grid.cell(0, 0).center();
    let mid_cell = box_grid.cell(1, 0).center();
    let bottom_cell = box_grid.cell(2, 0).center();

    let eq1 = await math_to_img(String.raw`
        \mathbf{T}_{raw} =

        \begin{bmatrix}
            1.0009 & 0.0047 & -0.0117 & -26.4924 \\
            0.0027 & -0.9996 & 0.0009 & 130.7906 \\
            -0.0034 & -0.0043 & -1.0041 & -13.0362
        \end{bmatrix}
    `);
    vid.add_object('math1', { opacity: 0 }, (ctx, params) => {
        ctx.translate(top_cell.x, top_cell.y);
        ctx.scale(1.5 * (1 / math_scale()), 1.5 * (1 / math_scale()));
        ctx.drawImage(eq1, -eq1.width / 2, -eq1.height / 2);
    })

    vid.add_object('math1_zero', { opacity: 0 }, (ctx, params) => {

        // TODO: 208 when viewing on the web.
        ctx.translate(top_cell.x + 180, top_cell.y);
        ctx.font = '40px "Noto Sans"';
        ctx.strokeStyle = '#fff';
        draw_box_text(ctx, 104, 98, '0');
    })

    let eq2 = await math_to_img(String.raw`
        \mathbf{T}_{final} = \mathbf{R}, \mathbf{\_} = RQ(\mathbf{T}_{raw})
    `);
    vid.add_object('math2', { opacity: 0 }, (ctx, params) => {
        ctx.translate(mid_cell.x, mid_cell.y);
        ctx.scale(1.5 * (1 / math_scale()), 1.5 * (1 / math_scale()));
        ctx.drawImage(eq2, -eq2.width / 2, -eq2.height / 2);
    });

    let eq3 = await math_to_img(String.raw`
        \mathbf{T}_{final} =
        \begin{bmatrix}
            1.0010 & -0.0019 & 0.0088 \\
            0 & 1.0011 & 0.0033 \\
            0 & 0 & 1.0040
        \end{bmatrix}
    `);
    vid.add_object('math3', { opacity: 0, scale: 1.5, y: bottom_cell.y }, (ctx, params) => {
        ctx.translate(bottom_cell.x, params.y);
        ctx.scale(params.scale * (1 / math_scale()), params.scale * (1 / math_scale()));
        ctx.drawImage(eq3, -eq3.width / 2, -eq3.height / 2);
    });

    let eq4 = await math_to_img(String.raw`    
        \mathbf{T}_{final}
        \cdot
        \begin{bmatrix}
        100 \\ 100 \\ 0
        \end{bmatrix}

        =

        \begin{bmatrix}
        99.9047 \\ 100.1119 \\ 0
        \end{bmatrix}
    `);
    vid.add_object('math4', { opacity: 0, scale: 1.5 }, (ctx, params) => {
        let cell = two_grid.cell(1, 0).center();
        ctx.translate(cell.x, cell.y);
        ctx.scale(params.scale * (1 / math_scale()), params.scale * (1 / math_scale()));
        ctx.drawImage(eq4, -eq4.width / 2, -eq4.height / 2);
    });

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'math1'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;

    vid.add_transition(['math1_zero'], t, 0.5, { opacity: 0.9 })
    t += 0.5;
    t += pause;

    vid.add_transition(['math2'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;

    vid.add_transition(['math3'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;

    vid.add_transition(['math1_zero', 'math1', 'math2'], t, 0.5, { opacity: 0 });
    vid.add_transition(['math3'], t, 0.5, { scale: 2, y: two_grid.cell(0, 0).center().y })
    t += 0.5;
    t += pause;

    vid.add_transition(['math4'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;


    vid.set_duration(t);

    return vid;
}

async function part5_math_video(canvas) {

    let vid = new Timeline();
    vid.set_name('part5_math');


    vid.add_object('title', { opacity: 0, text: 'Skew Math' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });


    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 1);

    let top_cell = box_grid.cell(0, 0).center();
    let mid_cell = box_grid.cell(1, 0).center();
    let bottom_cell = box_grid.cell(2, 0).center();

    let eq1 = await math_to_img('\\mathbf{P}_{machine} = \\mathbf{T} \\cdot \\mathbf{P}_{real}');
    vid.add_object('math1', { opacity: 0 }, (ctx, params) => {
        ctx.translate(top_cell.x, top_cell.y);
        ctx.scale(2 * (1 / math_scale()), 2 * (1 / math_scale()));
        ctx.drawImage(eq1, -eq1.width / 2, -eq1.height / 2);
    })

    vid.add_object('arrow12', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            top_cell.x, top_cell.y + 40,
            mid_cell.x, mid_cell.y - 60,
            2, 20, false
        );
    })

    let eq2 = await math_to_img(String.raw`
        \begin{bmatrix}
        x_m \\
        y_m \\
        z_m
        \end{bmatrix}
        =
        \begin{bmatrix}
        a & b & c & d \\
        e & f & g & h \\
        i & j & k & l
        \end{bmatrix}
        \begin{bmatrix}
        x_r \\
        y_r \\
        z_r \\
        1
        \end{bmatrix}
    `);
    vid.add_object('math2', { opacity: 0 }, (ctx, params) => {
        ctx.translate(mid_cell.x, mid_cell.y);
        ctx.scale(1.5 * (1 / math_scale()), 1.5 * (1 / math_scale()));
        ctx.drawImage(eq2, -eq2.width / 2, -eq2.height / 2);
    });


    let eq2_alt = await math_to_img(String.raw`
        \begin{bmatrix}
        x_{m1} & x_{m2} & \cdots \\
        y_{m1} & y_{m2} & \cdots \\
        z_{m1} & z_{m2} & \cdots
        \end{bmatrix}
        =
        \begin{bmatrix}
        a & b & c & d \\
        e & f & g & h \\
        i & j & k & l
        \end{bmatrix}
        \begin{bmatrix}
        x_{r1} & x_{r2} & \cdots \\
        y_{r1} & y_{r2} & \cdots \\
        z_{r1} & z_{r2} & \cdots \\
        1 & 1 & \cdots
        \end{bmatrix}
    `);
    vid.add_object('math2_alt', { opacity: 0 }, (ctx, params) => {
        ctx.translate(mid_cell.x, mid_cell.y);
        ctx.scale(1.5 * (1 / math_scale()), 1.5 * (1 / math_scale()));
        ctx.drawImage(eq2_alt, -eq2_alt.width / 2, -eq2_alt.height / 2);
    })

    vid.add_object('arrow23', { opacity: 0 }, (ctx, params) => {
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        drawArrow(
            ctx,
            mid_cell.x, mid_cell.y + 60,
            bottom_cell.x, bottom_cell.y - 40,
            2, 20, false
        );
    })

    let eq3 = await math_to_img('\\mathbf{T} = \\mathbf {P}_{machine} \\cdot (\\mathbf{P}_{real})^{-1}');
    vid.add_object('math3', { opacity: 0 }, (ctx, params) => {
        ctx.translate(bottom_cell.x, bottom_cell.y);
        ctx.scale(2 * (1 / math_scale()), 2 * (1 / math_scale()));
        ctx.drawImage(eq3, -eq3.width / 2, -eq3.height / 2);
    });


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'math1'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;

    vid.add_transition(['arrow12', 'math2'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;

    vid.add_transition(['math2'], t, 0.5, { opacity: 0 })
    vid.add_transition(['math2_alt'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;

    vid.add_transition(['arrow23', 'math3'], t, 0.5, { opacity: 1 })
    t += 0.5;
    t += pause;


    vid.set_duration(t);

    return vid;

}

function part5_triangulation_video(canvas) {
    let vid = new Timeline();
    vid.set_name('part5_triangulation');

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;


    vid.add_object('title', { opacity: 0, text: 'Camera Triangulation' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let bed_width = 700;
    let sheet_height = 4;
    let bed_height = 40;
    let layer_height = 6;

    let pattern_num_boxes = 11;
    let pattern_left = centerX - (bed_width / 2);
    let pattern_interval = bed_width / pattern_num_boxes;

    let pattern_height = 10;
    let pattern_top = 448 - (pattern_height / 2);

    let pattern_points = [];
    for (var i = 1; i < pattern_num_boxes; i++) {
        pattern_points.push({ x: pattern_left + pattern_interval * i, y: pattern_top })

    }

    vid.add_object('bed', { opacity: 0 }, (ctx, params) => {

        ctx.translate(centerX, 450);

        ctx.fillStyle = '#888';
        ctx.translate(0, (sheet_height / 2) + (layer_height / 2));
        draw_box(ctx, bed_width, sheet_height);

        ctx.fillStyle = '#ddd';
        ctx.font = '20px "Noto Sans"';
        ctx.translate(0, (sheet_height / 2) + (bed_height / 2));
        draw_box_text(ctx, bed_width, bed_height, 'Bed');
    });

    let toolhead = new DiagramBox({
        text: 'Toolhead',
        width: 200,
        height: 180,
        font_size: 20,
        text_offset: { x: 0, y: -105 },
        position: { x: centerX, y: 200 }
    });

    let offset_x = 0;
    let offset_y = 0;
    vid.add_object('offset', { opacity: 1, x: 0, y: 0 }, (ctx, params) => {
        offset_x = params.x;
        offset_y = params.y;

    });

    vid.add_object('toolhead', { opacity: 0 }, (ctx, params) => {
        ctx.translate(offset_x, offset_y);
        toolhead.draw(ctx);
    })


    vid.add_object('pattern', { opacity: 0 }, (ctx, params) => {


        for (var i = 0; i < pattern_num_boxes; i++) {
            ctx.save();
            ctx.translate(pattern_left + pattern_interval * (i + 0.5), 448);

            if (i % 2 == 1) {
                ctx.fillStyle = '#000';
            } else {
                ctx.fillStyle = '#fff';
            }

            draw_box(ctx, pattern_interval, pattern_height);

            ctx.restore();
        }
    })



    let image_width = 80;
    let camera_center = { x: centerX, y: 200 };

    vid.add_object('camera', { opacity: 0 }, (ctx, params) => {

        ctx.fillStyle = '#ddf';
        ctx.strokeStyle = '#000';

        ctx.translate(centerX, 200);

        ctx.translate(offset_x, offset_y);

        ctx.beginPath();
        ctx.moveTo(0, 0);
        ctx.lineTo(image_width / 2, 80);
        ctx.lineTo(-image_width / 2, 80);
        ctx.lineTo(0, 0);
        ctx.fill();
        ctx.stroke();

        draw_box(ctx, 80, 80);
    });

    vid.add_object('rays', { opacity: 1, lines: 0 }, (ctx, params) => {
        ctx.restore();

        ctx.save();
        pattern_points.map((pt) => {

            let camera_center_now = shallow_copy(camera_center);
            camera_center_now.x += offset_x;
            camera_center_now.y += offset_y;

            let pixel_pt = getPointAtY(camera_center_now, pt, camera_center_now.y + 80);
            if (pixel_pt.x < camera_center_now.x - (image_width / 2) ||
                pixel_pt.x > camera_center_now.x + (image_width / 2)) {
                return;
            }

            ctx.setLineDash([5, 5]);
            drawPolyline(ctx, [
                pt,
                camera_center_now
            ], params.lines * 0.5);

        });
        ctx.restore();
    })

    vid.add_object('triangle1', { opacity: 1, t: 0 }, (ctx, params) => {

        let a = shallow_copy(pattern_points[4]);
        a.y -= 2

        let b = shallow_copy(pattern_points[5]);
        b.y -= 2;

        let pts = [
            camera_center,
            a,
            b,
        ];

        ctx.lineWidth = 3;
        ctx.strokeStyle = 'rgba(42, 191, 211, 1)';
        drawPolyline(ctx, pts, params.t);
    })

    vid.add_object('triangle2', { opacity: 1, t: 0 }, (ctx, params) => {

        let a = getPointAtY(camera_center, pattern_points[4], camera_center.y + 80);
        let b = getPointAtY(camera_center, pattern_points[5], camera_center.y + 80);

        let pts = [
            camera_center,
            a,
            b,
        ];

        ctx.lineWidth = 3;
        ctx.strokeStyle = '#ff34ffff';
        drawPolyline(ctx, pts, params.t);
    })

    vid.add_object('pattern_points', { opacity: 0 }, (ctx, params) => {

        pattern_points.map((pt, i) => {
            ctx.save();

            ctx.fillStyle = '#f00';
            ctx.beginPath();
            ctx.arc(pt.x, pt.y, 3, 0, 2 * Math.PI);
            ctx.fill();

            ctx.translate(pt.x - 5, pt.y - 10);
            draw_multiline_text(ctx, {
                text: `(${i * 5}, 0)`,
                font_size: 15,
                text_align: 'right',
                color: '#f00'
            })

            ctx.restore();
        })
    });

    vid.add_object('pixels', { opacity: 0 }, (ctx, params) => {
        pattern_points.map((pt) => {
            let camera_center_now = shallow_copy(camera_center);
            camera_center_now.x += offset_x;
            camera_center_now.y += offset_y;


            let pixel_pt = getPointAtY(camera_center_now, pt, camera_center_now.y + 80);
            if (pixel_pt.x < camera_center_now.x - (image_width / 2) ||
                pixel_pt.x > camera_center_now.x + (image_width / 2)) {
                return;
            }

            ctx.fillStyle = '#f00';
            ctx.beginPath();
            ctx.arc(pixel_pt.x, pixel_pt.y, 3, 0, 2 * Math.PI);
            ctx.fill();

        });
    });

    vid.add_object('pixel_pos', { opacity: 0 }, (ctx, params) => {

        let a = getPointAtY(camera_center, pattern_points[4], camera_center.y + 80);
        let b = getPointAtY(camera_center, pattern_points[5], camera_center.y + 80);

        ctx.save();
        ctx.translate(a.x - 10, a.y + 20);
        draw_multiline_text(ctx, {
            text: `Pixel #2`,
            font_size: 20,
            text_align: 'right',
            color: '#f00'
        })
        ctx.restore();

        ctx.save();
        ctx.translate(b.x + 10, b.y + 20);
        draw_multiline_text(ctx, {
            text: `Pixel #7`,
            font_size: 20,
            text_align: 'left',
            color: '#f00'
        })
        ctx.restore();

    });


    let image_plane_y = camera_center.y + 80;
    let image_plane_right = camera_center.x + (image_width / 2) + 10;
    vid.add_object('image_plane_label', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#f00';
        ctx.strokeStyle = '#f00';


        drawArrow(
            ctx,
            image_plane_right + 80, image_plane_y,
            image_plane_right, image_plane_y,

            2, 20, false
        );

        ctx.save();
        ctx.translate(image_plane_right + 90, image_plane_y,);
        draw_multiline_text(ctx, {
            text: 'Image Plane',
            text_align: 'left',
            color: '#f00'
        })
        ctx.restore();
    });

    vid.add_object('optical_center_label', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#0a0';
        ctx.strokeStyle = '#0a0';

        drawArrow(
            ctx,
            image_plane_right + 80, camera_center.y,
            camera_center.x + 10, camera_center.y,
            2, 20, false
        );

        ctx.translate(image_plane_right + 90, camera_center.y);
        draw_multiline_text(ctx, {
            text: 'Camera Center',
            text_align: 'left',
            color: '#0a0'
        })

    })

    vid.add_object('focal_length', { opacity: 0 }, (ctx, params) => {
        let pt1 = { x: camera_center.x - 50, y: camera_center.y };
        let pt2 = { x: pt1.x, y: pt1.y + 80 };

        ctx.strokeStyle = '#00f'
        ctx.beginPath();
        ctx.moveTo(pt1.x, pt1.y);
        ctx.lineTo(pt1.x - 130, pt1.y);
        ctx.stroke();


        ctx.moveTo(pt1.x - 120, pt1.y);
        ctx.lineTo(pt1.x - 120, pt2.y);
        ctx.stroke();

        ctx.moveTo(pt1.x - 130, pt2.y);
        ctx.lineTo(pt1.x, pt2.y);
        ctx.stroke();



        ctx.translate(pt1.x - 135, (pt1.y + pt2.y) / 2);
        draw_multiline_text(ctx, {
            text: 'Focal Length',
            text_align: 'right',
            color: '#00f'
        });
    });

    vid.add_object('extra_dots', { opacity: 1, offsets: [], shear: 0 }, (ctx, params) => {

        params.offsets.map((offset) => {
            ctx.save();
            ctx.translate(offset.x, offset.y);

            // Machine positions
            ctx.fillStyle = 'rgba(0, 17, 170, 1)';
            ctx.beginPath();
            ctx.arc(camera_center.x, camera_center.y - 40, 8, 0, 2 * Math.PI);
            ctx.fill();
            ctx.stroke();

            // real position
            ctx.translate(params.shear * 0.2 * (offset.y + 20), 0);

            ctx.fillStyle = 'rgba(0, 170, 43, 1)';
            ctx.beginPath();
            ctx.arc(camera_center.x, camera_center.y, 8, 0, 2 * Math.PI);
            ctx.fill();
            ctx.stroke();


            ctx.restore();
        })

    });

    vid.add_object('opt_center', { opacity: 0 }, (ctx, params) => {
        ctx.translate(offset_x, offset_y);

        ctx.fillStyle = 'rgba(0, 170, 43, 1)';
        ctx.beginPath();
        ctx.arc(camera_center.x, camera_center.y, 8, 0, 2 * Math.PI);
        ctx.fill();
        ctx.stroke();
    });

    vid.add_object('opt_center_label', { opacity: 0 }, (ctx, params) => {

        ctx.fillStyle = 'rgba(0, 170, 43, 1)';
        ctx.strokeStyle = 'rgba(0, 170, 43, 1)';
        drawArrow(
            ctx,
            camera_center.x + 120, camera_center.y - 30,
            camera_center.x + 15, camera_center.y - 10,
            2, 20, false
        );


        ctx.translate(camera_center.x + 130, camera_center.y - 30);

        draw_multiline_text(ctx, {
            text: 'Estimated "Real" Position\n(22.5, 100)',
            text_align: 'left',
            color: 'rgba(0, 170, 43, 1)'
        })
    });

    let machine_pos_base = {
        x: camera_center.x,
        y: camera_center.y - 60
    };

    vid.add_object('machine_pos', { opacity: 0 }, (ctx, params) => {
        ctx.translate(offset_x, offset_y);

        ctx.fillStyle = 'rgba(0, 17, 170, 1)';
        ctx.beginPath();
        ctx.arc(camera_center.x, camera_center.y - 40, 8, 0, 2 * Math.PI);
        ctx.fill();
        ctx.stroke();
    });

    vid.add_object('machine_pos_label', { opacity: 0 }, (ctx, params) => {

        ctx.fillStyle = 'rgba(0, 17, 170, 1)';
        ctx.strokeStyle = 'rgba(0, 17, 170, 1)';
        drawArrow(
            ctx,
            camera_center.x - 120, camera_center.y - 40,
            camera_center.x - 15, camera_center.y - 40,
            2, 20, false
        );


        ctx.translate(camera_center.x - 130, camera_center.y - 40);

        draw_multiline_text(ctx, {
            text: 'Machine Position\n(x, y)',
            text_align: 'right',
            color: 'rgba(0, 17, 170, 1)'
        })
    });


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['toolhead', 'title', 'bed'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('camera', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('pattern', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('image_plane_label', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('rays', t, 0.5, { lines: 1 });
    vid.add_transition('optical_center_label', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition('pixels', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    // TODO: Hide labels
    vid.add_transition(['optical_center_label', 'image_plane_label'], t, 0.5, { opacity: 0 });
    t += 0.5;
    t += pause;


    // Show two triangles.
    vid.add_transition('triangle1', t, 1, { t: 1 });
    t += 1;
    vid.add_transition('triangle2', t, 1, { t: 1 });
    t += 1;
    t += pause;

    // Show the (x,y) pattern ppitns
    vid.add_transition('pattern_points', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;



    // Show pixel 'x' points
    vid.add_transition('pixel_pos', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    // show focal length label
    vid.add_transition('focal_length', t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['opt_center', 'opt_center_label'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['machine_pos', 'machine_pos_label'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;



    vid.add_transition(['machine_pos_label', 'opt_center_label', 'triangle2', 'triangle1', 'focal_length', 'pixel_pos', 'pattern_points'], t, 0.5, { opacity: 0 });
    t += 0.5;
    t += pause;


    let grid_offsets = [
        { x: 0, y: 0 }, { x: 100, y: 0 }, { x: 200, y: 0 },
        { x: 200, y: 100 }, { x: 100, y: 100 }, { x: 0, y: 100 },
        { x: -100, y: 100 },
        { x: -200, y: 100 },
        { x: -200, y: 0 },
        { x: -100, y: 0 },
    ]

    let current_offsets = [];
    grid_offsets.map((offset) => {

        vid.add_transition('offset', t, 0.5, { x: offset.x, y: offset.y });
        t += 0.5;
        t += pause;

        current_offsets.push(offset);
        // TODO: Ensure the add_key_frame function always internally deep copies stuff.
        vid.add_key_frame('extra_dots', t, { offsets: current_offsets.slice() });

    })

    vid.add_transition(['toolhead', 'camera', 'rays', 'pixels', 'machine_pos', 'opt_center'], t, 0.5, { opacity: 0 });
    t += 0.5;
    t += pause;

    vid.add_transition('extra_dots', t, 0.5, { shear: 1 });
    t += 0.5;
    t += pause;

    vid.set_duration(t);

    return vid;
}

////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////////////

function part2_shrinkage_video(canvas) {
    let vid = new Timeline();
    vid.set_name('part2_shrinkage');


    vid.add_object('title', { opacity: 0, text: 'Shrinkage' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(1, 3);

    let label_offset = 170;

    vid.add_object('square', { opacity: 0, t: 0, v: 0 }, (ctx, params) => {
        let pos = box_grid.cell(0, 1).center();


        let mm = 20 - params.v * 1;
        mm = Math.round(10 * mm) / 10;

        let temp = Math.round((100 - (params.v * 75)));

        ctx.save();
        ctx.translate(pos.x, pos.y + label_offset);
        draw_multiline_text(ctx, {
            text: `Temperature: ${temp} C`
        });
        ctx.restore();

        let size = 200 - 25 * params.v;
        let shear_x = 0;
        let curvature_x = 0;
        let progress = params.t;

        let text = `${mm}mm`;
        drawShearedSquare(ctx, pos, size, shear_x, curvature_x, progress, '#0bf', text);

        ctx.globalAlpha *= (1 - params.v);
        drawShearedSquare(ctx, pos, size, shear_x, curvature_x, progress, '#f00', text);


    });

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'square'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    vid.add_transition('square', t, 1, { t: 1 });
    t += 1;

    t += pause;

    vid.add_transition('square', t, 2, { v: 1 });
    t += 2;

    t += pause;

    vid.set_duration(t);

    return vid;

}

function part2_skew_video(canvas) {

    let vid = new Timeline();
    vid.set_name('part2_skew');

    vid.add_object('title', { opacity: 0, text: 'Skew Types' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(1, 3);

    let label_offset = 170;

    vid.add_object('noskew', { opacity: 0, t: 0 }, (ctx, params) => {
        let pos = box_grid.cell(0, 0).center();
        let size = 200;
        let shear_x = 0;
        let curvature_x = 0;
        let progress = params.t;
        let color = '#0bf';

        drawShearedSquare(ctx, pos, size, shear_x, curvature_x, progress, color);

        ctx.translate(pos.x, pos.y + label_offset);
        draw_multiline_text(ctx, {
            text: 'No\nSkew'
        });
    });

    vid.add_object('linearskew', { opacity: 0, t: 0 }, (ctx, params) => {
        let pos = box_grid.cell(0, 1).center();
        let size = 200;
        let shear_x = 30 * params.t;
        let curvature_x = 0;
        let progress = 1;
        let color = '#f00';

        drawShearedSquare(ctx, pos, size, shear_x, curvature_x, progress, color);

        ctx.translate(pos.x, pos.y + label_offset);
        draw_multiline_text(ctx, {
            text: 'Linear\nShear'
        });
    });

    vid.add_object('nonlinearskew', { opacity: 0, t: 0 }, (ctx, params) => {
        let pos = box_grid.cell(0, 2).center();
        let size = 200;
        let shear_x = 30;
        let curvature_x = 20 * params.t;
        let progress = 1;
        let color = '#0f8';

        drawShearedSquare(ctx, pos, size, shear_x, curvature_x, progress, color);

        ctx.translate(pos.x, pos.y + label_offset);
        draw_multiline_text(ctx, {
            text: 'Non-Linear\nSkew'
        });
    });

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    ['noskew', 'linearskew', 'nonlinearskew'].map((name) => {
        vid.add_transition([name], t, 0.5, { opacity: 1 });
        t += 0.5;

        t += pause;

        vid.add_transition([name], t, 1, { t: 1 });
        t += 1;

        t += pause;
    })

    vid.set_duration(t);

    return vid;
}