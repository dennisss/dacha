import { Timeline, draw_title, deg2rad, draw_box, slide_body_grid, DiagramBox, WireBundle, Wire, shallow_copy } from '../../utils.js';
import { hexToRgba } from '../../hex_to_rgba.js';
import { drawArrow } from '../../arrow.js';
import { interpolateValue } from '../../linear_interp.js';
import { approximateCurve, processChunks } from '../../regress.js';
import { STAIRCASE_DATA, TRAPEZOID_DATA, TRAPEZOID_DATA2, CORNERING1, CORNERING2 } from './motion_animation_data.js';

const POS_COLOR = '#0bf';
const NEG_COLOR = '#f20';

function draw_chamfered_box(ctx, width, height, chamfer) {
    ctx.save();

    let x = -width / 2;
    let y = -height / 2;

    ctx.beginPath();
    // Top edge
    ctx.moveTo(x + chamfer, y);
    ctx.lineTo(x + width - chamfer, y);

    // Right top corner
    ctx.lineTo(x + width, y + chamfer);

    // Right edge
    ctx.lineTo(x + width, y + height - chamfer);

    // Right bottom corner
    ctx.lineTo(x + width - chamfer, y + height);

    // Bottom edge
    ctx.lineTo(x + chamfer, y + height);

    // Left bottom corner
    ctx.lineTo(x, y + height - chamfer);

    // Left edge
    ctx.lineTo(x, y + chamfer);

    ctx.closePath();

    ctx.fillStyle = '#ddd';  // Blue fill
    ctx.strokeStyle = '#000'; // Dark blue stroke
    ctx.lineWidth = 2;

    // Apply fill and stroke
    ctx.fill();
    ctx.stroke();

    ctx.restore();
}

export class StepperMotor {

    constructor(params) {
        this._position = params.position;
        this._size = 250;

        this._hollow_radius = 90;

        this._num_coils = 8;
        this._step_size = 360 / this._num_coils;
    }

    draw(ctx, params) {
        ctx.save();

        ctx.translate(this._position.x, this._position.y);


        draw_chamfered_box(ctx, this._size, this._size, 10);

        // opacity: 0, hollow_opacity: 0, magnet_opacity: 0, title_opacity: 1, coil_opacity: 0

        {
            ctx.save();

            ctx.globalAlpha *= params.hollow_opacity;

            ctx.beginPath();
            ctx.arc(0, 0, this._hollow_radius, 0, 2 * Math.PI);

            ctx.lineWidth = 2;
            ctx.fillStyle = '#fff';
            ctx.strokeStyle = '#000';
            ctx.fill();
            ctx.stroke();

            ctx.restore();
        }

        // Coils
        {
            for (var i = 0; i < 8; i++) {
                let power = 0;
                if (i % 2 == 0) {
                    power = params.a_coil_power;
                } else {
                    power = params.b_coil_power;
                }

                if ((Math.floor(i / 2) % 2) == 1) {
                    power = -power;
                }

                let color = '#fff';
                if (power > 0.1) {
                    color = hexToRgba(POS_COLOR, power);
                } else if (power < -0.1) {
                    color = hexToRgba(NEG_COLOR, -power);
                }


                ctx.save();

                ctx.globalAlpha *= params.coil_opacity;

                ctx.rotate(deg2rad(-i * (360 / 8)));

                ctx.translate(this._hollow_radius + 10, 0);

                ctx.lineWidth = 2;
                ctx.fillStyle = '#fff';
                ctx.strokeStyle = '#000';
                draw_box(ctx, 30, 30);

                ctx.lineWidth = 2;
                ctx.fillStyle = color;
                ctx.strokeStyle = '#000';
                draw_box(ctx, 30, 30);

                ctx.restore();
            }
        }

        {
            ctx.save();

            ctx.globalAlpha *= params.magnet_opacity;

            ctx.rotate(deg2rad(params.shaft_angle));

            this._draw_rotor(ctx, params);

            ctx.restore();
        }

        // Shaft
        {
            let shaft_radius = 8;

            ctx.save();

            ctx.beginPath();
            ctx.arc(0, 0, shaft_radius, 0, 2 * Math.PI);

            ctx.lineWidth = 2;
            ctx.fillStyle = '#ccc';
            ctx.strokeStyle = '#000';
            ctx.fill();
            ctx.stroke();

            ctx.restore();
        }



        {
            ctx.save();

            ctx.globalAlpha *= params.title_opacity;

            ctx.font = `20px "Noto Sans"`;
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText("Motor", 0, -40);

            ctx.restore();
        }


        ctx.restore();
    }

    _draw_rotor(ctx, params) {
        let magnet_height = 30;
        let magnet_width = 2 * this._hollow_radius - 20;

        // Magnet pole
        {
            ctx.save();

            ctx.fillStyle = '#0bf';
            ctx.fillRect(
                0,
                -(magnet_height / 2),
                magnet_width / 2,
                magnet_height
            );

            ctx.restore();
        }

        // Magnet
        {
            ctx.save();

            ctx.fillStyle = 'rgba(0,0,0,0)';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            draw_box(ctx, magnet_width, magnet_height);


            ctx.restore();
        }

        {
            ctx.save();

            ctx.globalAlpha *= params.finger_opacity;

            ctx.translate(magnet_width / 2 - 20, magnet_height / 2 + 30);
            ctx.scale(-1, 1);
            ctx.rotate(deg2rad(-90));



            ctx.textDrawingMode = "glyph";
            ctx.font = '50px "Noto Color Emoji"';
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';

            ctx.shadowColor = '#000';
            ctx.shadowBlur = 8;

            ctx.fillText('👉', 0, 0);

            ctx.restore();
        }

    }

    left_center() {
        return {
            x: this._position.x - (this._size / 2),
            y: this._position.y
        }
    }
}

class WireGraph {
    constructor(initial_value, propagation_delay) {
        this._initial_value = initial_value;
        this._propagation_delay = propagation_delay;
        this._flips = [];
        this._sorted = true;
        this._inverted = false;
    }

    clear() {
        this._flips = [];
        this._sorted = true;
    }

    invert() {
        this._inverted = true;
    }

    flip(time, value) {
        this._flips.push({ time, value });
        this._sorted = false;
    }

    to_points(current_time) {

        /*
        At the current time we show:
        
        - at x: 0 => point at time = current_time
        - at x: 1 => point at time = current_time - propagation_delay

        */

        if (!this._sorted) {
            this._flips.sort((a, b) => {
                if (a.time == b.time) {
                    throw new Error('Duplicate points');
                }

                return a.time - b.time;
            });

            this._sorted = true;
        }

        let time_to_x = (t) => {
            let x = (t - current_time) / -this._propagation_delay;
            if (this._inverted) {
                x = 1 - x;
            }

            return x;
        }

        // let time

        let points = [];

        points.push({
            x: time_to_x(0),
            y: this._initial_value
        });

        let last_value = this._initial_value;

        // TODO: This is currently fairly inefficient and relies on the drawing code for clipping.

        for (var i = 0; i < this._flips.length; i++) {
            let x = time_to_x(this._flips[i].time);
            let y = this._flips[i].value;

            points.push({
                x,
                y: last_value
            });
            points.push({
                x,
                y
            });

            last_value = y;
        }

        points.push({
            x: time_to_x(current_time + 1),
            y: last_value
        });

        // console.log(points);

        return points;
    }



}

/*
Part 6 Animations:

- Hide everything

- show a timer bxo


- to the right of the timer is a 'Counter' box.

- configure timer

- Show counting

- Pan counter to the bottom of the screen

- Make the current timer value 'blue'

- Draw boxes left and right
    - Other boxes are partially greyed out
    - Some text showing 'Past'
    - 'Future'

- Middle bar represents the CPU

- Big vertical line indicating the current time.





*/


export function configure(canvas) {
    // return part3_video(canvas, true);
    // return part5_video(canvas, true);
    // return part6_video(canvas);
    // return part7_video(canvas);
    // return part8_video(canvas);
    // return part8_video2(canvas);
    // return part8_video3(canvas);
    // return part9_video(canvas);
    return part9_video2(canvas);

}


function part9_video2(canvas) {
    let second_data = false;
    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'Cornering' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let input_data = second_data ? CORNERING2 : CORNERING1;

    let data1_max_time = 6;

    let max_pos = 400;

    let data1 = input_data.trim().split('\n')
        .map((row) => {
            let cols = row.trim().split(',');
            return {
                time: cols[0] * 1,
                x: cols[1] * 1,
                dx: cols[2] * 1
            };
        })


    let grid = slide_body_grid(canvas).split(2, 1);

    vid.add_object('graph1', { opacity: 0, max: 0 }, (ctx, params) => {

        let cell1 = grid.cell(0, 0);
        let pos1 = cell1.bottom_left();
        ctx.translate(pos1.x, pos1.y);

        let ft = (t) => {
            if (t > params.max) {
                return;
            }

            let time = t * data1_max_time;

            let y = interpolateValue(data1, time, 'x') / max_pos;

            return {
                x: t,
                y
            };
        }

        draw_graph(ctx, {
            width: cell1.width(),
            height: cell1.height() - 20,
            y_label: 'Position',
            x_label: 'Time',
            color: '#0bf',
            font_size: 20
        }, ft);


    });

    vid.add_object('graph2', { opacity: 0, max: 0 }, (ctx, params) => {

        let cell1 = grid.cell(1, 0);
        let pos1 = cell1.bottom_left();
        ctx.translate(pos1.x, pos1.y);

        let ft = (t) => {
            if (t > params.max) {
                return;
            }

            let time = t * data1_max_time;

            let y = interpolateValue(data1, time, 'dx') / 150;

            return {
                x: t,
                y
            };
        }

        draw_graph(ctx, {
            width: cell1.width(),
            height: cell1.height() - 20,
            y_label: 'Velocity',
            x_label: 'Time',
            color: '#f00',
            font_size: 20
        }, ft);


    });

    vid.add_object('jerk', { opacity: 0 }, (ctx) => {
        let x = 387;
        let y = 456;

        ctx.moveTo(x, y);
        ctx.lineTo(x + 100, y);
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 2;
        ctx.setLineDash([5, 5]);
        ctx.stroke();


        ctx.translate(x - 5, y);

        ctx.font = `20px "Noto Sans Mono"`;
        ctx.fillStyle = '#f00';
        ctx.textAlign = 'right';
        ctx.textBaseline = 'middle';
        ctx.fillText('Max "Jerk"', 0, 0);
    });

    // vid.add_object('feedrate', { opacity: 0 }, (ctx) => {
    //     ctx.translate(300, 370);

    //     ctx.font = `20px "Noto Sans Mono"`;
    //     ctx.fillStyle = '#f00';
    //     ctx.textAlign = 'middle';
    //     ctx.textBaseline = 'center';
    //     ctx.fillText('Feed Rate', 0, 0);
    // });


    let t = 0;

    let pause = 1;

    vid.add_transition(['title', 'graph1', 'graph2'], t, 0.5, { opacity: 1 });

    if (second_data) {
        vid.add_transition(['jerk'], t, 0.5, { opacity: 1 });
    }

    t += 0.5;

    vid.add_transition(['graph1', 'graph2'], t, 5, { max: data1_max_time });
    t += 5;


    // vid.add_transition('graph')

    /*
    if (second_data) {
    
        vid.add_transition(['graph1', 'graph2'], t, 3, { max: 1 });
        t += 3;
    
    } else {
    
    
        vid.add_transition(['graph1', 'graph2'], t, 1, { max: 1 / 6 });
        t += 1;
    
        t += pause;
    
        vid.add_transition(['accel'], t, 0.5, { opacity: 1 });
        t += 0.5;
    
        t += pause;
    
        vid.add_transition(['graph1', 'graph2'], t, 4, { max: 5 / 6 });
        t += 1;
    
        vid.add_transition(['feedrate'], t, 0.5, { opacity: 1 });
        t += 3;
    
        t += pause;
    
        vid.add_transition(['graph1', 'graph2'], t, 1, { max: 6 / 6 });
        t += 1;
    }
    
    
    t += pause;
    */


    vid.set_duration(t);


    return vid;

}



function part9_video(canvas) {
    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'Circling' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let grid = slide_body_grid(canvas);

    let graph_size = Math.min(grid._width, grid._height);

    vid.add_object('graph1', { opacity: 0, max: 0 }, (ctx, params) => {
        let bottom_left = grid.center();
        bottom_left.y += graph_size / 2;
        bottom_left.x -= graph_size / 2;


        ctx.translate(bottom_left.x, bottom_left.y);

        // let center = 

        let num_sides = 8;

        let points = [];
        for (var i = 0; i <= num_sides; i++) {
            let a = 2 * Math.PI * (i / num_sides);

            // Same as below.
            points.push({ x: 0.5 + 0.4 * Math.cos(a), y: 0.5 + 0.4 * Math.sin(a) });
        }

        draw_graph(ctx, {
            width: graph_size,
            height: graph_size,
            // color: '#0bf',
            x_label: 'X',
            y_label: 'Y',
            series: [
                {
                    color: '#0bf',
                    f: (t) => {
                        let a = 2 * Math.PI * t;
                        return { x: 0.5 + 0.4 * Math.cos(a), y: 0.5 + 0.4 * Math.sin(a) }
                    }

                },
                {
                    color: '#f00',
                    f: (t) => {
                        if (t > params.max) {
                            return null
                        }

                        let idx = t * (points.length - 1);

                        let alpha = 1 - (Math.ceil(idx) - idx)

                        let next = Math.ceil(idx);
                        let prev = Math.floor(idx);

                        return {
                            x: points[next].x * alpha + points[prev].x * (1 - alpha),
                            y: points[next].y * alpha + points[prev].y * (1 - alpha),
                        }


                        //

                    }
                }

            ]

        })
    })

    let t = 0;

    let pause = 1;

    vid.add_transition(['title', 'graph1'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    vid.add_transition(['graph1'], t, 2, { max: 1 });
    t += 2;

    t += pause;

    /*
    vid.add_transition(['graph1'], t, 2, { max: 1 });
    t += 2;

    t += pause;


    vid.add_transition(['graph2'], t, 0.5, { opacity: 1 });
    vid.add_transition(['graph1'], t, 0.01, { max: 0 });
    t += pause;

    // vid.set_start_time(t);

    vid.add_transition(['graph1', 'graph2'], t, 2, { max: 1 });
    t += 2;

    t += pause;
    */

    vid.set_duration(t);


    return vid;


}

function part8_video3(canvas) {
    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'Calculating Step Times' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let input_data = SAMPLE_RAMP;

    let data1_max_time = 0.1;

    let max_pos = 400;

    let data1 = input_data.trim().split('\n')
        .map((row) => {
            let cols = row.trim().split(',');
            return {
                time: cols[0] * 1,
                x: cols[1] * 1,
                dx: cols[2] * 1
            };
        })

    let step_times = SAMPLE_STEP_TIMES.trim().split('\n')
        .map((row) => {
            let cols = row.trim().split(',');
            return {
                time: cols[0] * 1,
                x: cols[1] * 1,
            };
        });

    let basic_step_times = [];

    for (var i = 1; i < 10; i++) {
        basic_step_times.push(processChunks(step_times, i, (step_times) => {
            let curve = approximateCurve(step_times.map((row) => row.time));

            let last_time = curve.time0;
            let last_dur = curve.duration0;
            let add = curve.add;
            let pos = step_times[0].x;

            let out = []
            while (pos <= step_times[step_times.length - 1].x) {
                out.push({ time: last_time, x: pos });

                last_time = last_time + last_dur;
                last_dur += add;
                pos += 1;
            }

            return out;
        }));
    }







    let grid = slide_body_grid(canvas).split(1, 1);
    let cell1 = grid.cell(0, 0);

    vid.add_object('graph1', { opacity: 0, max: 0, graph_opacity: 1, depth: 0, times_opacity: 0, approx_opacity: 0 }, (ctx, params) => {

        let pos1 = cell1.bottom_left();
        ctx.translate(pos1.x, pos1.y);

        let ft = (t) => {
            if (t > params.max) {
                return;
            }

            let time = t * data1_max_time;

            let y = interpolateValue(data1, time, 'x') / max_pos;

            return {
                x: t,
                y
            };
        }

        ctx.save();
        draw_graph(ctx, {
            width: cell1.width(),
            height: cell1.height() - 20,
            y_label: 'Position',
            x_label: 'Time',
            color: hexToRgba('#0bf', params.graph_opacity),
            font_size: 20
        }, ft);
        ctx.restore();



        step_times.map((row, i) => {

            if (i % 8 != 0) {
                return;
            }

            let x = (row.time / data1_max_time) * cell1.width();
            let y = -(row.x / max_pos) * (cell1.height() - 20)

            x += 3;
            y -= 3;

            let radius = 3;
            ctx.beginPath();
            ctx.arc(x, y, radius, 0, Math.PI * 2); // Draw a full circle
            ctx.fillStyle = hexToRgba('#f00', params.times_opacity);
            ctx.fill();
            ctx.closePath();

        })



        {
            let interval = 1 / (Math.ceil(params.depth) + 1);
            let i = interval;

            while (i < 1) {
                ctx.beginPath()

                ctx.setLineDash([5, 5]);
                ctx.strokeStyle = '#0bf';
                ctx.lineWidth = 2;

                let y = -i * (cell1.height() - 20);

                ctx.moveTo(3, y);
                ctx.lineTo(cell1.width(), y);
                ctx.stroke();


                i += interval;
            }

        }




        basic_step_times[0].map((row, i) => {

            let depth = params.depth;

            let alpha = 1 - (Math.ceil(depth) - depth);
            let time = basic_step_times[Math.ceil(depth)][i].time * alpha
                + basic_step_times[Math.floor(depth)][i].time * (1 - alpha);



            if (i % 8 != 0) {
                return;
            }

            let x = (time / data1_max_time) * cell1.width();
            let y = -(row.x / max_pos) * (cell1.height() - 20)

            x += 3;
            y -= 3;

            let radius = 3;
            ctx.beginPath();
            ctx.arc(x, y, radius, 0, Math.PI * 2); // Draw a full circle
            ctx.fillStyle = hexToRgba('#0f0', params.approx_opacity);
            ctx.fill();
            ctx.closePath();

        })

    });


    let t = 0;

    let pause = 1.0;

    vid.add_transition(['title', 'graph1'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['graph1'], t, 1, { max: 1 });
    t += 1;
    t += pause;

    // vid.set_start_time(t);

    vid.add_transition(['graph1'], t, 0.5, { times_opacity: 1 });
    t += 0.5;
    vid.add_transition(['graph1'], t, 0.5, { graph_opacity: 0 });
    t += 0.5;
    t += pause;

    vid.add_transition(['graph1'], t, 0.5, { approx_opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['graph1'], t, 1, { depth: 1 });
    t += 1;
    t += pause;

    vid.add_transition(['graph1'], t, 4, { depth: 5 });
    t += 4;
    t += pause;

    vid.set_duration(t);

    return vid;
}



function part8_video2(canvas) {

    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'Calculating Step Times' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let grid = slide_body_grid(canvas).split(3, 1);

    vid.add_object('1', { opacity: 0 }, (ctx) => {
        let pos = grid.cell(0, 0).left_center();
        pos.y -= 10;

        ctx.translate(pos.x, pos.y);

        ctx.font = `25px "Noto Sans"`;
        ctx.fillStyle = '#000';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'bottom';
        ctx.fillText('1. Constant Velocity', 0, 0);

        ctx.font = `20px "Noto Sans Mono"`;
        ctx.textBaseline = 'top';
        ctx.fillText('next_step_time = last_step_time + velocity', 30, 20);
    });

    vid.add_object('2', { opacity: 0 }, (ctx) => {
        let pos = grid.cell(1, 0).left_center();
        pos.y -= 10;

        ctx.translate(pos.x, pos.y);

        ctx.font = `25px "Noto Sans"`;
        ctx.fillStyle = '#000';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'bottom';
        ctx.fillText('2. Constant Acceleration', 0, 0);

        ctx.font = `20px "Noto Sans Mono"`;
        ctx.textBaseline = 'top';
        ctx.fillText('step_time = (-start_velocity + sqrt(start_velocity^2 + 2*accel*position))', 30, 20);
        ctx.fillText(' / accel', 160, 45);
    });

    vid.add_object('3', { opacity: 0 }, (ctx) => {
        let pos = grid.cell(2, 0).left_center();
        pos.y -= 10;

        ctx.translate(pos.x, pos.y);

        ctx.font = `25px "Noto Sans"`;
        ctx.fillStyle = '#000';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'bottom';
        ctx.fillText('3. "Klipper" / Quadratic Motion', 0, 0);

        ctx.font = `20px "Noto Sans Mono"`;
        ctx.textBaseline = 'top';
        ctx.fillText('step_time = last_step_time + last_step_duration', 30, 20);
        ctx.fillText('step_duration = last_step_duration + add', 30, 45);
    });

    let t = 0;

    let pause = 1.0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    vid.add_transition(['1'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['2'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['3'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;


    vid.set_duration(t);

    return vid;

}

function part8_video(canvas) {
    let second_data = true;
    let third_data = false;

    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: third_data ? 'Staircase Acceleration' : 'Smooth Motion' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let input_data = second_data ? TRAPEZOID_DATA2 : TRAPEZOID_DATA;

    let data1_max_time = second_data ? 1.414 : 6;

    let max_pos = second_data ? 50 : 500;

    if (third_data) {

        input_data = STAIRCASE_DATA;
        data1_max_time = 10;
        max_pos = 550;
    }


    let data1 = input_data.trim().split('\n')
        .map((row) => {
            let cols = row.trim().split(',');
            return {
                time: cols[0] * 1,
                x: cols[1] * 1,
                dx: cols[2] * 1
            };
        })


    let grid = slide_body_grid(canvas).split(2, 1);

    vid.add_object('graph1', { opacity: 0, max: 0 }, (ctx, params) => {

        let cell1 = grid.cell(0, 0);
        let pos1 = cell1.bottom_left();
        ctx.translate(pos1.x, pos1.y);

        let ft = (t) => {
            if (t > params.max) {
                return;
            }

            let time = t * data1_max_time;

            let y = interpolateValue(data1, time, 'x') / max_pos;

            return {
                x: t,
                y
            };
        }

        draw_graph(ctx, {
            width: cell1.width(),
            height: cell1.height() - 20,
            y_label: 'Position',
            x_label: 'Time',
            color: '#0bf',
            font_size: 20
        }, ft);


    });

    vid.add_object('graph2', { opacity: 0, max: 0 }, (ctx, params) => {

        let cell1 = grid.cell(1, 0);
        let pos1 = cell1.bottom_left();
        ctx.translate(pos1.x, pos1.y);

        let ft = (t) => {
            if (t > params.max) {
                return;
            }

            let time = t * data1_max_time;

            let y = interpolateValue(data1, time, 'dx') / 150;

            return {
                x: t,
                y
            };
        }

        draw_graph(ctx, {
            width: cell1.width(),
            height: cell1.height() - 20,
            y_label: 'Velocity',
            x_label: 'Time',
            color: '#f00',
            font_size: 20
        }, ft);


    });

    vid.add_object('accel', { opacity: 0 }, (ctx) => {
        ctx.translate(50, 470);
        ctx.rotate(deg2rad(-40));

        ctx.font = `20px "Noto Sans Mono"`;
        ctx.fillStyle = '#f00';
        ctx.textAlign = 'middle';
        ctx.textBaseline = 'center';
        ctx.fillText('Acceleration', 0, 0);
    });

    vid.add_object('feedrate', { opacity: 0 }, (ctx) => {
        ctx.translate(300, 370);

        ctx.font = `20px "Noto Sans Mono"`;
        ctx.fillStyle = '#f00';
        ctx.textAlign = 'middle';
        ctx.textBaseline = 'center';
        ctx.fillText('Feed Rate', 0, 0);
    });


    let t = 0;

    let pause = 1;

    vid.add_transition(['title', 'graph1', 'graph2'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    if (second_data) {

        vid.add_transition(['graph1', 'graph2'], t, 3, { max: 1 });
        t += 3;

    } else {


        vid.add_transition(['graph1', 'graph2'], t, 1, { max: 1 / 6 });
        t += 1;

        t += pause;

        vid.add_transition(['accel'], t, 0.5, { opacity: 1 });
        t += 0.5;

        t += pause;

        vid.add_transition(['graph1', 'graph2'], t, 4, { max: 5 / 6 });
        t += 1;

        vid.add_transition(['feedrate'], t, 0.5, { opacity: 1 });
        t += 3;

        t += pause;

        vid.add_transition(['graph1', 'graph2'], t, 1, { max: 6 / 6 });
        t += 1;
    }


    t += pause;


    vid.set_duration(t);


    return vid;

}

export function draw_graph(ctx, params, ft) {

    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(params.width, 0);
    ctx.moveTo(0, 1.5);
    ctx.lineTo(0, -params.height);
    ctx.strokeStyle = '#666';
    ctx.lineWidth = 3;
    ctx.stroke();

    let font_size = params.font_size || 30;

    ctx.font = `${font_size}px "Noto Sans Mono"`;
    ctx.fillStyle = '#444';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'bottom';

    ctx.fillText(params.x_label, params.width - 10, -3);

    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillText(params.y_label, 5, -params.height + 2);

    if (!params.series) {
        params.series = [{
            color: params.color,
            f: ft
        }];
    }


    let num_steps = 400;

    let width = params.width;
    let height = params.height;

    params.series.map((series) => {
        ctx.lineWidth = 2;
        ctx.strokeStyle = series.color;

        ctx.beginPath();
        for (var i = 0; i < num_steps; i++) {
            let t = i / num_steps;

            let pos = (series.f)(t)
            if (pos === null || pos === undefined) {
                break;
            }

            let x = pos.x;
            let y = pos.y;

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
}


function part7_video(canvas) {


    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'Core XY' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });


    let grid = slide_body_grid(canvas).split(1, 2);

    let cell1 = grid.cell(0, 0);
    let cell2 = grid.cell(0, 1);

    let graph_size = Math.min(cell1._width, cell1._height);

    function xy_square(t) {
        let base_x = 0;
        let base_y = 0;

        let size = 0.6;

        if (t < 0.25) {
            return {
                x: base_x + size,
                y: base_y + size - ((t / 0.25) * size),

            }
        }

        if (t < 0.5) {
            return {
                x: base_x + size - ((t - 0.25) / 0.25) * size,
                y: base_y,
            }
        }

        if (t < 0.75) {
            return {
                x: base_x,
                y: base_y + ((t - 0.5) / 0.25) * size
            }
        }

        return {
            x: base_x + ((t - 0.75) / 0.25) * size,
            y: base_y + size
        };
    }

    vid.add_object('graph1', { opacity: 0, max: 0 }, (ctx, params) => {
        let bottom_left = cell1.left_center();
        bottom_left.y += graph_size / 2;

        ctx.translate(bottom_left.x, bottom_left.y);

        let fx = (t) => {
            if (t > params.max) {
                return null;
            }

            let p = xy_square(t);
            return {
                x: 0.2 + p.x,
                y: 0.2 + p.y
            }
        }

        draw_graph(ctx, {
            graph_size,
            color: '#0bf',
            x_label: 'X',
            y_label: 'Y'
        }, fx)
    })

    vid.add_object('graph2', { opacity: 0, max: 0 }, (ctx, params) => {
        let bottom_left = cell2.right_center();
        bottom_left.y += graph_size / 2;
        bottom_left.x -= graph_size;

        ctx.translate(bottom_left.x, bottom_left.y);

        let fx = (t) => {
            if (t > params.max) {
                return null;
            }

            let p = xy_square(t);
            return {
                x: (p.x + p.y) / 1.5 + 0.1,
                y: -(p.x - p.y) / 1.5 + 0.5
            }
        }

        draw_graph(ctx, {
            graph_size,
            color: '#f00',
            x_label: 'A',
            y_label: 'B'
        }, fx)

    })

    let t = 0;

    let pause = 1;

    vid.add_transition(['title', 'graph1'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    vid.add_transition(['graph1'], t, 2, { max: 1 });
    t += 2;

    t += pause;


    vid.add_transition(['graph2'], t, 0.5, { opacity: 1 });
    vid.add_transition(['graph1'], t, 0.01, { max: 0 });
    t += pause;

    // vid.set_start_time(t);

    vid.add_transition(['graph1', 'graph2'], t, 2, { max: 1 });
    t += 2;

    t += pause;

    vid.set_duration(t);


    return vid;

}



function draw_spinner(ctx) {
    ctx.save();

    let radius = 10;

    ctx.beginPath();
    ctx.arc(0, 0, radius, 0, Math.PI * 1.6);
    ctx.lineWidth = 4;
    ctx.strokeStyle = '#444';
    ctx.stroke();

    ctx.restore();
}

function part6_video(canvas) {
    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'First Steps' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);

    let counter = new DiagramBox({
        text: '0',
        width: 150,
        height: 75,
        font_size: 25,
        position: body_grid.center(),
        font_family: 'Noto Sans Mono'
    });

    let timer_pos = body_grid.center();
    timer_pos.x -= 300;

    let timer = new DiagramBox({
        text: 'Timer',
        width: 150,
        height: 75,
        font_size: 22,
        position: timer_pos,
    });

    let interrupt_pos = body_grid.center();
    interrupt_pos.x += 300;

    let interrupt = new DiagramBox({
        text: '',
        width: 150,
        height: 75,
        font_size: 25,
        position: interrupt_pos,
        font_family: 'Noto Sans Mono'
    })

    let cpu = new DiagramBox({
        text: 'Microcontroller CPU',
        width: 800,
        height: 130,
        font_size: 20,
        text_offset: { x: 0, y: -40 },
        position: body_grid.center()
    });

    vid.add_object('cpu', { opacity: 0 }, (ctx, params) => {
        cpu.draw(ctx);
    });

    let step_task_pos = body_grid.center();
    step_task_pos.x += 250;
    step_task_pos.y += 10;

    let spinner_angle = 0;

    let spinners = vid.add_object('spinners', { angle: 0, opacity: 1 }, (ctx, params) => {
        spinner_angle = params.angle;
    })
    vid.add_key_frame('spinners', 1000, { angle: 2 * Math.PI * 1000 });

    let step_task = new DiagramBox({
        text: 'Stepping Code',
        width: 250,
        height: 75,
        position: step_task_pos,
        text_offset: { x: -20, y: 0 }
    });
    vid.add_object('step_task', { opacity: 0, running: true }, (ctx, params) => {
        step_task.set_background_color(hexToRgba('#fff', 1 - params.opacity));
        step_task.draw(ctx);

        if (params.running) {
            ctx.translate(step_task.right_center().x - 30, step_task.position().y - 2);
            ctx.rotate(spinner_angle);
            draw_spinner(ctx);
        }


    });

    let other_task_pos = body_grid.center();
    other_task_pos.x -= 250;
    other_task_pos.y += 10;

    let other_task = new DiagramBox({
        text: 'USB Receiver',
        width: 250,
        height: 75,
        position: other_task_pos,
        text_offset: { x: -20, y: 0 }
    });
    vid.add_object('other_task', { opacity: 0, running: true }, (ctx, params) => {
        other_task.set_background_color(hexToRgba('#fff', 1 - params.opacity));
        other_task.draw(ctx);

        if (params.running) {
            ctx.translate(other_task.right_center().x - 30, other_task.position().y - 2);
            ctx.rotate(spinner_angle);
            draw_spinner(ctx);
        }
    });

    let step_pin_pos = body_grid.center();
    step_pin_pos.y -= 160;

    let step_pin = new DiagramBox({
        text: 'Step Pin\nValue = 0',
        width: 150,
        height: 75,
        font_size: 20,
        position: step_pin_pos
    });
    vid.add_object('step_pin', { opacity: 0, value: false }, (ctx, params) => {
        if (!params.value) {
            step_pin.set_background_color('#fff');
            step_pin.set_text_color('#000');
            step_pin.set_text('Step Pin\n(Low)');
        } else {
            step_pin.set_background_color('#000');
            step_pin.set_text_color('#fff');
            step_pin.set_text('Step Pin\n(High)');
        }

        step_pin.draw(ctx);
    });

    vid.add_object('step_arrow', {}, (ctx, params) => {
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        drawArrow(
            ctx,
            step_task.top_center().x, step_task.top_center().y,
            step_pin.right_center().x, step_pin.bottom_center().y,
            2, 20, false
        );
    });



    let sleeping_alpha = 0.1;

    function get_alpha(v) {
        return sleeping_alpha + (Math.sin(v * Math.PI / 2)) * (1 - sleeping_alpha);
    }


    // Setup all positions before anything else runs.
    vid.add_object('timer_row', { offset: 0, opacity: 1 }, (ctx, params) => {
        {
            let pos = body_grid.center();
            pos.y += params.offset;
            counter._position = pos;
        }

        {
            let pos = shallow_copy(timer_pos);
            pos.y += params.offset;
            timer._position = pos;
        }

        {
            let pos = shallow_copy(interrupt_pos);
            pos.y += params.offset;
            interrupt._position = pos;
        }
    })


    vid.add_object('counter', { opacity: 0, text: '0' }, (ctx, params) => {


        counter.set_text(params.text);
        counter.draw(ctx);
    });

    vid.add_object('timer', { opacity: 0, alpha: 0.1, arrow_opacity: 0, text: 'Timer' }, (ctx, params) => {


        let alpha = get_alpha(params.alpha);
        timer.set_text(params.text);
        timer.set_background_color(`rgba(170, 204, 238, ${alpha})`);
        timer.draw(ctx);

        let arrow_alpha = alpha * params.arrow_opacity;

        let c = `rgba(0,0,0,${arrow_alpha})`;
        ctx.fillStyle = c;
        ctx.strokeStyle = c;
        drawArrow(
            ctx,
            timer.right_center().x, timer.right_center().y,
            counter.left_center().x, counter.left_center().y,
            2, 20, false
        );

    });

    vid.add_object('interrupt', { opacity: 0, alpha: 0.1, arrow_opacity: 0, text: '= X?', arrow_aligned: false }, (ctx, params) => {

        {
            ctx.fillStyle = '#000';
            ctx.fillStyle = '#000';
            drawArrow(
                ctx,
                counter.right_center().x, counter.right_center().y,
                interrupt.left_center().x, interrupt.left_center().y,
                2, 20, false
            );
        }


        let alpha = get_alpha(params.alpha);
        interrupt.set_text(params.text);
        interrupt.set_background_color(`rgba(170, 204, 238, ${alpha})`);
        interrupt.draw(ctx);


        let arrow_alpha = alpha * params.arrow_opacity;

        let end_pos = { x: interrupt.top_center().x, y: cpu.bottom_center().y };
        if (params.arrow_aligned) {
            end_pos = step_task.bottom_center();
        }

        let c = `rgba(0,0,0,${arrow_alpha})`;
        ctx.fillStyle = c;
        ctx.strokeStyle = c;
        drawArrow(
            ctx,
            interrupt.top_center().x, interrupt.top_center().y,
            end_pos.x, end_pos.y,
            2, 20, false
        );

    });

    vid.add_object('compare_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        drawArrow(
            ctx,

            step_task.bottom_center().x - 30, step_task.bottom_center().y,
            interrupt.top_center().x - 30, interrupt.top_center().y,
            2, 20, false
        );
    })

    vid.add_object('interrupt_spinner', { opacity: 0 }, (ctx, params) => {
        ctx.translate(interrupt.top_center().x + 15, interrupt.top_center().y - 15);
        ctx.rotate(spinner_angle);
        draw_spinner(ctx);
    })

    let ppi_pos = { x: interrupt_pos.x, y: step_pin.position().y };

    let ppi = new DiagramBox({
        text: 'nRF52 PPI',
        width: 150,
        height: 75,
        font_size: 20,
        position: ppi_pos
    });
    vid.add_object('ppi', { opacity: 0 }, (ctx, params) => {
        ppi.draw(ctx);

        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';

        drawArrow(
            ctx,
            ppi.left_center().x, ppi.left_center().y,
            step_pin.right_center().x, step_pin.right_center().y,
            2, 20, false
        );

        ctx.beginPath();

        let a = interrupt.right_center();
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(a.x + 60, a.y);
        ctx.lineTo(a.x + 60, ppi.position().y);
        ctx.stroke();

        drawArrow(
            ctx,
            a.x + 60, ppi.position().y,
            ppi.right_center().x, ppi.right_center().y,
            2, 20, false
        );

    })


    let t = 0;

    let pause = 1;

    vid.add_transition(['title', 'timer'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    vid.add_transition(['timer'], t, 0.5, { arrow_opacity: 1 });
    vid.add_transition(['counter'], t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    vid.add_key_frame('timer', t, { text: '16MHz\nTimer' });
    t += pause;

    let counter_value = 0;

    for (var i = 0; i < 10; i++) {
        vid.add_transition('timer', t, 0.2, { alpha: 1 });
        t += 0.2;

        counter_value += 1;
        vid.add_key_frame('counter', t, { text: counter_value + '' });

        vid.add_transition('timer', t, 0.2, { alpha: 0.1 });
        t += 0.2;
    }

    t += pause;

    vid.add_transition('interrupt', t, 0.5, { opacity: 1 });
    t += 0.5;

    vid.add_transition(['timer_row'], t, 0.5, { offset: 140 });
    t += 0.5;

    vid.add_transition('interrupt', t, 0.5, { arrow_opacity: 1 });
    vid.add_transition('cpu', t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    let step_pin_value = false;
    let last_step_time = 10;

    let trigger_step_code = (pause) => {
        vid.add_key_frame('step_task', t, { running: true });
        vid.add_transition('step_task', t, 0.5, { opacity: 1 });
        t += 0.5;

        t += pause;

        vid.add_transition('step_pin', t, 0.5, { opacity: 1 });
        t += 0.5;

        vid.add_transition('step_arrow', t, 0.5, { opacity: 1 });
        t += 0.5;

        step_pin_value = !step_pin_value;
        vid.add_key_frame('step_pin', t, { value: step_pin_value });

        vid.add_transition('step_arrow', t, 0.5, { opacity: 0 });
        t += 0.5;

        t += pause;

        vid.add_transition('compare_arrow', t, 0.5, { opacity: 1 });
        t += 0.5;

        let next_time = last_step_time + 8000;
        vid.add_key_frame('interrupt', t, { arrow_aligned: true, text: `= ${next_time}?` });
        last_step_time = next_time;

        vid.add_transition('compare_arrow', t, 0.5, { opacity: 0 });
        t += 0.5;

        t += pause;

        vid.add_key_frame('step_task', t, { running: false });
        vid.add_transition('step_task', t, 0.5, { opacity: 0.4 });
        t += 0.5;
    };

    trigger_step_code(pause);

    for (var i = 0; i < 5; i++) {
        vid.add_transition('timer', t, 0.2, { alpha: 1 });
        t += 0.2;

        counter_value += 1;
        vid.add_key_frame('counter', t, { text: counter_value + '' });

        vid.add_transition('timer', t, 0.2, { alpha: 0.1 });
        t += 0.2;
    }

    while (counter_value < 8005) {
        counter_value += 1;
        vid.add_key_frame('counter', t, { text: counter_value + '' });
        t += 0.0001;
    }

    for (var i = 0; i < 5; i++) {
        vid.add_transition('timer', t, 0.2, { alpha: 1 });
        t += 0.2;

        counter_value += 1;
        vid.add_key_frame('counter', t, { text: counter_value + '' });

        vid.add_transition('timer', t, 0.2, { alpha: 0.1 });
        t += 0.2;
    }

    vid.add_transition('interrupt', t, 0.5, { alpha: 1 });
    t += 0.5;

    trigger_step_code(0.1);

    vid.add_transition('interrupt', t, 0.5, { alpha: 0 });
    t += 0.5;


    t += pause;

    vid.add_transition('other_task', t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    while (counter_value < 16005) {
        counter_value += 1;
        vid.add_key_frame('counter', t, { text: counter_value + '' });
        t += 0.0001;
    }

    for (var i = 0; i < 5; i++) {
        vid.add_transition('timer', t, 0.2, { alpha: 1 });
        t += 0.2;

        counter_value += 1;
        vid.add_key_frame('counter', t, { text: counter_value + '' });

        vid.add_transition('timer', t, 0.2, { alpha: 0.1 });
        t += 0.2;
    }

    vid.add_transition('interrupt', t, 0.5, { alpha: 1 });
    vid.add_transition('interrupt_spinner', t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;

    for (var i = 0; i < 5; i++) {
        vid.add_transition('timer', t, 0.2, { alpha: 1 });
        t += 0.2;

        counter_value += 1;
        vid.add_key_frame('counter', t, { text: counter_value + '' });

        vid.add_transition('timer', t, 0.2, { alpha: 0.1 });
        t += 0.2;
    }

    vid.add_key_frame('other_task', t, { running: false });
    vid.add_transition('other_task', t, 0.5, { opacity: 0.4 });
    t += 0.5;

    t += pause;

    vid.add_transition('interrupt_spinner', t, 0.5, { opacity: 0 });
    trigger_step_code(0.1);

    vid.add_transition('interrupt', t, 0.5, { alpha: 0.1 });
    t += 0.5;

    t += pause;


    vid.add_transition('ppi', t, 0.5, { opacity: 1 });
    t += 0.5;

    t += pause;



    vid.set_duration(t);

    return vid;
}

function part5_video(canvas, part6 = false) {
    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'Microcontroller Interfacing' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(1, 3);

    let host_pos = box_grid.cell(0, 0).left_center();
    host_pos.x += 150 / 2 + 30;

    let host = new DiagramBox({
        text: 'Raspberry Pi\n(or computer)',
        width: 150,
        height: 300,
        font_size: 20,
        position: host_pos
    })

    vid.add_object('host', { opacity: 0, subtitle: '', text: 'Raspberry Pi\n(or computer)' }, (ctx, params) => {
        host.set_text(params.text);

        host.draw(ctx);

        {
            ctx.textDrawingMode = "glyph";
            ctx.font = '30px "Noto Color Emoji"';
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';

            ctx.shadowColor = '#000';
            ctx.shadowBlur = 8;

            ctx.fillText(params.subtitle, host_pos.x, host_pos.y + 50);
        }

    })

    let mcu_pos = box_grid.cell(0, 2).right_center();
    mcu_pos.x -= 300 / 2 + 30;

    let mcu = new DiagramBox({
        text: 'Microcontroller Board',
        width: 300,
        height: 300,
        font_size: 20,
        position: mcu_pos
    })

    vid.add_object('mcu', { opacity: 0, title_y: 0, text: 'Microcontroller Board', subtitle: '' }, (ctx, params) => {
        mcu._text_offset.y = params.title_y;

        mcu.set_text(params.text);

        mcu.draw(ctx);


        {
            ctx.textDrawingMode = "glyph";
            ctx.font = '30px "Noto Color Emoji"';
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';

            ctx.shadowColor = '#000';
            ctx.shadowBlur = 8;

            ctx.fillText(params.subtitle, mcu.position().x, mcu.position().y + 40);
        }
        // 

    });


    let mcu_cpu = new DiagramBox({
        text: 'CPU',
        width: 100,
        height: 100,
        font_size: 30,
        position: mcu_pos
    });
    vid.add_object('mcu_cpu', { opacity: 0, offset_x: 0 }, (ctx, params) => {
        // mcu._text_offset.y = params.title_y;
        let pos = shallow_copy(mcu_pos);
        pos.x += params.offset_x;
        mcu_cpu._position = pos;

        mcu_cpu.draw(ctx);
    });


    let first_wire_bundle = new WireBundle({
        from: host.right_center(),
        to: mcu.left_center(),
        spacing: 20
    });

    first_wire_bundle.add_wire(new Wire({
        line_width: 20
    }));

    vid.add_object('first_wires', { height: 0, title: 'USB', opacity: 0, to_x: mcu.left_center().x }, (ctx, params) => {
        let to = mcu.left_center();
        to.x = params.to_x;
        first_wire_bundle.set_to(to);

        first_wire_bundle.wires()[0].title = params.title;
        first_wire_bundle.wires()[0].height = params.height;

        first_wire_bundle.draw(ctx);
    });


    vid.add_object('data_transfer', { opacity: 0, text: '', left: 0, align: 'left' }, (ctx, params) => {

        ctx.font = '20px "Noto Sans Mono"';
        ctx.fillStyle = '#fff';
        ctx.textAlign = params.align;
        ctx.textBaseline = 'middle';


        let left = host.right_center().x;
        let right = mcu.left_center().x;

        ctx.beginPath();
        ctx.rect(left, 0, right - left, 1000);
        ctx.clip();


        ctx.fillText(params.text, left + (params.left * (right - left)), host.right_center().y);

    })

    let uart_bundle = new WireBundle({
        from: host.right_center(),
        to: mcu_cpu.left_center(),
        spacing: 30
    });

    for (var i = 0; i < 2; i++) {
        uart_bundle.add_wire(new Wire({
            line_width: 2
        }));
    }

    vid.add_object('uart_wires', { titles: true, opacity: 0, from_x: host.right_center().x, to_x: mcu_cpu.left_center().x }, (ctx, params) => {
        uart_bundle.wires().map((wire, i) => {
            if (params.titles) {
                wire.title = i == 0 ? 'UART TX' : 'UART RX';
            } else {
                wire.title = ''
            }
        })

        let to = mcu_cpu.left_center();
        to.x = params.to_x;
        uart_bundle.set_to(to);

        let from = host.right_center();
        from.x = params.from_x;
        uart_bundle._from = from;

        uart_bundle.draw(ctx);
    });


    let adapter_pos = shallow_copy(mcu_pos);
    adapter_pos.x -= 60;

    let serial_adapter = new DiagramBox({
        text: 'USB\n->\nSerial',
        width: 100,
        height: 100,
        font_size: 18,
        position: adapter_pos
    });
    vid.add_object('serial_adapter', { opacity: 0 }, (ctx, params) => {
        serial_adapter.draw(ctx);
    });



    let pause = 1;
    let t = 0;

    vid.add_transition(['mcu', 'host', 'title'], t, 0.5, { opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition(['first_wires'], t, 0.5, { opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition('mcu', t, 0.5, { title_y: -120 });
    vid.add_transition('mcu_cpu', t, 0.5, { opacity: 1 })
    vid.add_transition('first_wires', t, 0.5, { to_x: mcu_cpu.left_center().x });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition('first_wires', t, 0.5, { opacity: 0 });
    vid.add_transition('uart_wires', t, 0.5, { opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_key_frame('uart_wires', t, { titles: false });

    vid.add_transition('first_wires', t, 0.5, { to_x: serial_adapter.left_center().x, title: 'USB' });
    vid.add_transition('mcu_cpu', t, 0.5, { offset_x: 70 });
    vid.add_transition('uart_wires', t, 0.5, {
        to_x: mcu_cpu.left_center().x + 70,
        from_x: serial_adapter.right_center().x
    });
    t += 0.5;

    vid.add_transition('serial_adapter', t, 0.5, { opacity: 1 });
    vid.add_transition('first_wires', t, 0.5, { opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;


    vid.add_transition(['mcu_cpu', 'uart_wires', 'serial_adapter'], t, 0.5, { opacity: 0 });
    vid.add_transition(['mcu'], t, 0.5, { title_y: 0 });
    vid.add_transition(['first_wires'], t, 0.5, { to_x: mcu.left_center().x, title: '' });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_key_frame('mcu', t, { text: 'Marlin/GRBL', subtitle: '💪' })
    vid.add_transition(['first_wires'], t, 0.5, { height: 50 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_key_frame('data_transfer', t, { text: '"G0 X1 Y2\\n"' });
    vid.add_transition('data_transfer', t, 0.5, { opacity: 1 });
    t += 0.5;

    vid.add_transition('data_transfer', t, 1, { left: 1 });
    t += 1;

    // Pause
    t += pause;

    vid.add_key_frame('data_transfer', t, { text: '"ok\\n"' });
    vid.add_transition('data_transfer', t, 1.3, { left: -0.3 });
    t += 1.3;

    // Pause
    t += pause;

    vid.add_key_frame('mcu', t, { text: 'Klipper MCU', subtitle: '' })
    vid.add_key_frame('host', t, { text: 'Klipper\nPi', subtitle: '💪' })

    // Pause
    t += pause;



    let random_data = '';
    for (var i = 0; i < 1000; i++) {
        if (Math.random() < 0.5) {
            random_data += '1'
        } else {
            random_data += '0';
        }
    }

    vid.add_key_frame('data_transfer', t, { align: 'right', text: random_data });

    vid.add_transition('data_transfer', t, 10, { left: 10 });
    t += 10;



    vid.add_key_frame('host', t, { text: 'My Pi\n(Client)', subtitle: '' });
    vid.add_key_frame('mcu', t, { text: 'My Microcontroller\n(Server)', subtitle: '' });
    vid.add_key_frame('data_transfer', t, { left: 0, text: ['[Request 3]  [Request 2]  [Request 1]'] });

    // Pause
    t += pause;


    vid.add_transition('data_transfer', t, 1.5 * 2.4, { left: 2.4 });
    t += 1.5 * 2.4;

    // Pause
    t += pause;

    vid.add_key_frame('data_transfer', t - 0.01, { left: 2.4, align: 'left', text: '[Response 2]  [Response 1]  [Response 3]' });
    vid.add_key_frame('data_transfer', t, { left: 1 });

    vid.add_transition('data_transfer', t, 1.5 * 2.4, { left: -1.4 });
    t += 1.5 * 2.4;


    /// Part 6 stuff.
    if (part6) {

        vid.set_start_time(t);

        vid.add_key_frame('host', t, { text: 'My Pi', subtitle: '' });
        vid.add_key_frame('mcu', t, { text: 'My Microcontroller', subtitle: '' });

        vid.add_key_frame('data_transfer', t, { opacity: 0 });

        vid.add_key_frame('title', t, { text: 'First Steps' });

        // Pause
        t += pause;

        vid.add_key_frame('data_transfer', t, { text: '"move motor 1 by 20mm"', left: 0.02 });
        vid.add_transition('data_transfer', t, 0.5, { opacity: 1, left: 0.02 });
        t += 0.5;

        // Pause
        t += 2 * pause;

        vid.add_transition('data_transfer', t, 0.5, { opacity: 0 });
        t += 0.5;

        vid.add_key_frame('data_transfer', t, { align: 'right', text: '"mm??"', left: 0.98 });
        vid.add_transition('data_transfer', t, 0.5, { opacity: 1 });
        t += 0.5;

        vid.add_transition('data_transfer', t, 2, { left: 0 });
        t += 2;

    }


    vid.set_duration(t);


    return vid;
}



function part3_video(canvas, part6 = false) {

    let vid = new Timeline();

    vid.add_object('title', { opacity: 0, text: 'Stepper Driver Wiring' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);

    let box_grid = body_grid.split(1, 3);

    let mcu_pos = box_grid.cell(0, 0).left_center();
    mcu_pos.x += 150 / 2 + 30;

    let mcu = new DiagramBox({
        text: 'MCU',
        width: 150,
        height: 300,
        font_size: 20,
        position: mcu_pos
    })

    let motor_driver = new DiagramBox({
        text: '',
        width: 100,
        height: 250,
        font_size: 20,
        position: box_grid.cell(0, 1).center()
    });

    let stepper = new StepperMotor({
        position: box_grid.cell(0, 2).center()
    });

    let motor_wire_bundle = new WireBundle({
        from: motor_driver.right_center(),
        to: stepper.left_center(),
        spacing: 20
    });

    let motor_wires = [];

    for (var i = 0; i < 4; i++) {
        let wire = new Wire({});
        motor_wires.push(wire);
        motor_wire_bundle.add_wire(wire);
    }

    /*
    Representing the wires.

    - initial value.

    - at time x, a value is generated on the MCU
    - after propagation delay
    - 

    - At time x, 

    */


    // vid.add_object('grid', { opacity: 0 }, (ctx) => {
    //     ctx.translate(body_grid.center().x, body_grid.center().y);
    //     draw_box(ctx, body_grid.width(), body_grid.height());
    // });



    vid.add_object('motor_wires', { opacity: 0, a_coil_power: 0, b_coil_power: 0 }, (ctx, params) => {
        // TODO: Color the wires based on the stepper power.

        motor_wires.map((wire, i) => {
            let power;
            if (i < 2) {
                power = params.a_coil_power
            } else {
                power = params.b_coil_power;
            }

            if (i % 2 == 1) {
                power = -power;
            }

            let color = '#000';
            let line_width = 2;

            if (power > 0.1) {
                color = POS_COLOR;
                line_width = 4;
            } else if (power < -0.1) {
                color = NEG_COLOR;
                line_width = 4;
            }


            wire.color = color;
            wire.line_width = line_width;
        })


        motor_wire_bundle.draw(ctx);
    })

    let shaft_angle = 20;

    vid.add_object('stepper', {
        opacity: 0,
        hollow_opacity: 0,
        magnet_opacity: 0,
        title_opacity: 1,
        coil_opacity: 0,
        a_coil_power: 0,
        b_coil_power: 0,
        shaft_angle: shaft_angle,
        finger_opacity: 0

    }, (ctx, params) => {
        stepper.draw(ctx, params);
    });


    let wire_prop_delay = 1;


    let mcu_wire_bundle = new WireBundle({
        from: mcu.right_center(),
        to: motor_driver.left_center(),
        spacing: 30
    })

    let mcu_wires = [];

    for (var i = 0; i < 5; i++) {
        let wire = new Wire({});
        mcu_wires.push(wire);
        mcu_wire_bundle.add_wire(wire);
    }


    let mcu_wire_graphs = [];
    for (var i = 0; i < mcu_wires.length; i++) {
        mcu_wire_graphs.push(new WireGraph(0, wire_prop_delay));
    }


    vid.add_object('mcu_wires', { expanded_height: 0, expanded_indexes: [], titles: [], time: 0 }, (ctx, params) => {
        mcu_wires.map((w, i) => {
            mcu_wires[i].title = params.titles[i];

            if (params.expanded_indexes.includes(i)) {
                mcu_wires[i].height = params.expanded_height;
            } else {
                mcu_wires[i].height = 0;
            }

            mcu_wires[i].graph = mcu_wire_graphs[i].to_points(params.time);
        });

        mcu_wire_bundle.draw(ctx);
    });
    // Hack to expose the absolute time to the object.
    if (!part6) {
        vid.add_key_frame('mcu_wires', 1000, { time: 1000 });
    }


    vid.add_object('mcu', { opacity: 0 }, (ctx) => {
        mcu.draw(ctx);
    })

    vid.add_object('mcu_exclaim', { opacity: 0 }, (ctx) => {

        ctx.translate(mcu_pos.x, mcu_pos.y + 60);

        ctx.textDrawingMode = "glyph";
        ctx.font = '50px "Noto Color Emoji"';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        ctx.fillText('😨', 0, 0);
    });

    vid.add_object('driver', { opacity: 0, text_color: '#000', text: 'Motor\nDriver' }, (ctx, params) => {
        motor_driver.set_text_color(params.text_color);
        motor_driver.set_text(params.text);
        motor_driver.draw(ctx);
    });


    let expanded_wire_height = 40;

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'stepper', 'mcu', 'driver'], t, 0.5, { opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;


    vid.add_transition(['motor_wires'], t, 0.5, { opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition(['stepper'], t, 0.5, { hollow_opacity: 1, title_opacity: 0 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition(['stepper'], t, 0.5, { magnet_opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition(['stepper'], t, 0.5, { coil_opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    let coil_power_objs = ['stepper', 'motor_wires'];

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 1 });
    t += 0.2;

    // Pause
    t += pause;


    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 1 });
    t += 0.2;

    // Pause
    t += pause;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 0 });
    t += 0.2;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 1, b_coil_power: 0 });
    t += 0.2;

    shaft_angle = 0;
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
    t += 0.5;

    for (var i = 0; i < 3; i++) {
        vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 1 });
        t += 0.2;

        shaft_angle -= stepper._step_size;
        vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
        t += 0.5;

        vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: -1, b_coil_power: 0 });
        t += 0.2;

        shaft_angle -= stepper._step_size;
        vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
        t += 0.5;

        vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: -1 });
        t += 0.2;

        shaft_angle -= stepper._step_size;
        vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
        t += 0.5;

        vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 1, b_coil_power: 0 });
        t += 0.2;

        shaft_angle -= stepper._step_size;
        vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
        t += 0.5;

    }

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 0 });

    // Pause
    t += pause;

    vid.add_transition('mcu_wires', t, 0.5, { opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_key_frame('mcu_wires', t, { expanded_indexes: [0], titles: ['UART'] });

    vid.add_transition('mcu_wires', t, 0.5, { expanded_height: expanded_wire_height });
    t += 0.5;

    {
        let duration = 1;
        let tn = t;
        let rate = 0.05;
        for (var i = 0; i < Math.floor(duration / rate); i++) {
            let v = Math.random() > 0.5;
            mcu_wire_graphs[0].flip(tn, v);
            tn += rate;
        }

        mcu_wire_graphs[0].flip(tn, 0);

        vid.add_key_frame('driver', t + 1, { text_color: '#00f' });
        vid.add_key_frame('driver', t + 1.25, { text_color: '#0ff' });
        vid.add_key_frame('driver', t + 1.5, { text_color: '#f00' });
        vid.add_key_frame('driver', t + 1.75, { text_color: '#f0f' });
        vid.add_key_frame('driver', t + 2, { text_color: '#000' });

        t += duration;

        t += wire_prop_delay;
    }

    // Pause
    t += pause;

    vid.add_transition('mcu_wires', t, 0.5, { expanded_height: 0, titles: [] });
    t += 0.5;

    mcu_wire_graphs[1].flip(t - 2, 1);
    vid.add_key_frame('mcu_wires', t, { expanded_indexes: [1], titles: ['', 'Enable'] });

    vid.add_transition('mcu_wires', t, 0.5, { expanded_height: expanded_wire_height });
    t += 0.5;

    // Pause
    t += pause;

    mcu_wire_graphs[1].flip(t, 0);
    t += wire_prop_delay;
    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 1, b_coil_power: 0 });

    // Pause
    t += pause;

    vid.add_transition('stepper', t, 0.25, { finger_opacity: 1 });
    t += 0.25;

    let tilted_angle = shaft_angle - (stepper._step_size / 2);
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle: tilted_angle });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition(['stepper'], t, 0.1, { finger_opacity: 0 });
    t += 0.1;
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
    t += 0.5;

    // Pause
    t += pause;



    vid.add_transition('stepper', t, 0.25, { finger_opacity: 1 });
    t += 0.25;

    let tilted_angle2 = shaft_angle - (3 * stepper._step_size);
    vid.add_transition(['stepper'], t, 1, { shaft_angle: tilted_angle2 });
    t += 1;

    vid.add_transition(['stepper'], t, 0.1, { finger_opacity: 0 });
    t += 0.1;

    shaft_angle -= 4 * stepper._step_size;
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition('mcu_wires', t, 0.5, { expanded_height: 0, titles: [] });
    t += 0.5;


    vid.add_key_frame('mcu_wires', t, { expanded_indexes: [2, 3], titles: ['', '', 'Direction (DIR)', 'Step'] });
    vid.add_transition('mcu_wires', t, 0.5, { expanded_height: expanded_wire_height });
    t += 0.5;

    // Pause
    t += pause;

    mcu_wire_graphs[3].flip(t, 1);
    mcu_wire_graphs[3].flip(t + 0.1, 0);
    t += 1;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 1 });
    shaft_angle -= stepper._step_size;
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
    t += 0.5;


    mcu_wire_graphs[2].flip(t, 1);
    t += 0.5;

    mcu_wire_graphs[3].flip(t, 1);
    mcu_wire_graphs[3].flip(t + 0.1, 0);
    t += 1;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 1, b_coil_power: 0 });
    shaft_angle += stepper._step_size;
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_key_frame('driver', t, { text: 'Motor\nDriver\n(DEDGE)' })

    mcu_wire_graphs[3].flip(t, 1);
    t += wire_prop_delay;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: -1 });
    shaft_angle += stepper._step_size;
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle });
    // t += 0.5; // Keep commented to allow next signal to overlap with motion.

    mcu_wire_graphs[3].flip(t, 0);
    t += wire_prop_delay;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: -1, b_coil_power: 0 });
    shaft_angle += stepper._step_size;
    vid.add_transition(['stepper'], t, 0.5, { shaft_angle });

    // Pause
    t += pause;

    vid.add_transition('mcu_wires', t, 0.5, { expanded_height: 0, titles: [] });
    t += 0.5;

    vid.add_key_frame('mcu_wires', t, { expanded_indexes: [4], titles: ['', '', '', '', 'DIAG'] });
    vid.add_transition('mcu_wires', t, 0.5, { expanded_height: expanded_wire_height });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_key_frame('driver', t, { text: 'Motor\nDriver\n(SGTHRS)' })

    // Pause
    t += pause;

    vid.add_transition(['stepper'], t, 0.5, { finger_opacity: 1 });
    t += 0.5;

    // Pause
    t += pause;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 1 });
    t += 0.7;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: -1, b_coil_power: 0 });
    t += 0.7;

    vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 1 });
    t += 0.7;


    mcu_wire_graphs[4].invert();
    mcu_wire_graphs[4].flip(t, 1);
    t += 1;

    vid.add_transition('mcu_exclaim', t, 0.2, { opacity: 1 });

    t += 1;


    vid.add_object('step_time', { opacity: 0 }, (ctx, params) => {

        let left = mcu.right_center().x;
        let y = mcu.right_center().y + 50;
        let right = motor_driver.left_center().x;

        let x1 = (right - left) * 0.2 + left - 1;
        let x2 = (right - left) * 0.4 + left - 1;

        ctx.beginPath();
        ctx.moveTo(x1, y);
        ctx.lineTo(x1, y + 150);

        ctx.moveTo(x2, y);
        ctx.lineTo(x2, y + 150);


        ctx.setLineDash([5, 5]);
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 2;
        ctx.stroke();


        ctx.fillStyle = 'red';
        ctx.font = '20px "Noto Sans"';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'bottom';

        ctx.fillText('0.5 milliseconds', x2 + 5, y + 150);
    });

    vid.add_object('step_velocity', { opacity: 0 }, (ctx, params) => {

        let left = mcu.right_center().x;
        let y = mcu.right_center().y + 8;
        let right = motor_driver.left_center().x;


        ctx.fillStyle = 'red';
        ctx.font = '20px "Noto Sans"';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';

        ctx.fillText('25 mm/s', (left + right) / 2, y);
    });


    /// Part 6 stuff
    // TODO: Make all these immediately changing.
    if (part6) {
        vid.set_start_time(t);

        vid.add_key_frame('title', t, { text: 'First Steps' });
        vid.add_key_frame('mcu_exclaim', t, { opacity: 0 });
        vid.add_key_frame('driver', t, { text: 'Motor\nDriver' });
        vid.add_key_frame('stepper', t, { finger_opacity: 0, shaft_angle: 0 });
        vid.add_key_frame(coil_power_objs, t, { a_coil_power: 0, b_coil_power: 0 });
        vid.add_key_frame('mcu_wires', t, { expanded_indexes: [3], titles: ['', '', '', 'Step'], expanded_height: expanded_wire_height });
        shaft_angle = 0;

        mcu_wire_graphs[3].clear();

        // Pause
        t += pause;

        vid.add_key_frame('mcu_wires', t, { time: 10 });


        let step_dur = 0.2;

        let tn = 10;
        for (var i = 0; i < 100; i++) {
            mcu_wire_graphs[3].flip(tn, (i + 1) % 2);
            tn += step_dur;
        }

        vid.add_key_frame('mcu_wires', t + (step_dur * 4 * 4), { time: 10 + (step_dur * 4 * 4) });

        t += wire_prop_delay;

        for (var i = 0; i < 3; i++) {
            vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: 1 });
            shaft_angle -= stepper._step_size;
            vid.add_transition(['stepper'], t, 0.2, { shaft_angle });
            t += 0.2;

            vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: -1, b_coil_power: 0 });
            shaft_angle -= stepper._step_size;
            vid.add_transition(['stepper'], t, 0.2, { shaft_angle });
            t += 0.2;

            vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 0, b_coil_power: -1 });
            shaft_angle -= stepper._step_size;
            vid.add_transition(['stepper'], t, 0.2, { shaft_angle });
            t += 0.2;

            vid.add_transition(coil_power_objs, t, 0.2, { a_coil_power: 1, b_coil_power: 0 });
            shaft_angle -= stepper._step_size;
            vid.add_transition(['stepper'], t, 0.2, { shaft_angle });
            t += 0.2;

        }

        // Pause
        t += pause;

        vid.add_transition('step_velocity', t, 0.5, { opacity: 1 });

        // Pause
        t += pause;

        vid.add_transition('step_time', t, 0.5, { opacity: 1 });

        // Pause
        t += pause;
    }




    vid.set_duration(t);

    return vid;
}