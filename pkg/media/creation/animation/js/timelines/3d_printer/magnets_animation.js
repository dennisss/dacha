import { Timeline, draw_title, deg2rad, draw_box, slide_body_grid, DiagramBox, WireBundle, Wire, shallow_copy, draw_multiline_text, draw_box_text } from '../../utils.js';
import { hexToRgba } from '../../hex_to_rgba.js';
import { drawArrow, drawArrowPos } from '../../arrow.js';
import { getPointAtY } from '../../y_point.js';
import { drawPolyline, drawSequentialChains, drawShearedSquare } from '../../sheared_square.js';
import { drawCenteredTable } from '../../centered_table.js';
import { draw_graph, WireGraph } from './motion_animation.js';
import { getInterpolatedY, interpolateValue } from '../../linear_interp.js';
import { getObjectAlpha } from '../../staggered_fade.js';

export async function configure(canvas) {
    // return part3_video(canvas);
    // return part4_video(canvas);
    return part5_video(canvas);
}


function part5_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part5");
    vid.add_object('title', { opacity: 0, text: 'Microcontroller Load Testing' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(1, 3);

    // Host / Pi (Left)
    let host_pos = box_grid.cell(0, -0.1).center();
    let host = new DiagramBox({
        text: 'Host / Pi',
        width: 150,
        height: 300,
        font_size: 20,
        position: host_pos
    });

    vid.add_object('host_box', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        host.draw(ctx);
        ctx.restore();
    });

    // MCU (Middle)
    let mcu_pos = box_grid.cell(0, 1).center();
    let mcu = new DiagramBox({
        text: 'MCU',
        width: 150,
        height: 300,
        font_size: 20,
        position: mcu_pos
    });

    vid.add_object('mcu_box', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        mcu.draw(ctx);
        ctx.restore();
    });

    // Motor Driver (Right)
    let driver_pos = box_grid.cell(0, 2.1).center();
    let driver = new DiagramBox({
        text: 'Motor Driver',
        width: 150,
        height: 300,
        font_size: 20,
        position: driver_pos
    });

    vid.add_object('driver_box', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        driver.draw(ctx);
        ctx.restore();
    });

    // Wires and Connections
    let usb_from = host.right_center();
    let usb_to = mcu.left_center();

    let usb_wire = new WireBundle({
        from: usb_from,
        to: usb_to,
        spacing: 0
    });
    usb_wire.add_wire(new Wire({ line_width: 8 }));

    vid.add_object('usb_wire', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        usb_wire.draw(ctx);
        ctx.restore();
    });

    let step_from = mcu.right_center();
    let step_to = driver.left_center();

    let step_wire = new WireBundle({
        from: step_from,
        to: step_to,
        spacing: 0
    });
    let step_line = new Wire({ line_width: 2 });
    step_line.title = 'STEP Pin';
    step_line.height = 60; // height to allow drawing graph
    step_wire.add_wire(step_line);

    let step_graph = new WireGraph(0, 0.5); // initial value, prop delay

    vid.add_object('step_wire_pulse', { opacity: 0, time: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;

        step_line.graph = step_graph.to_points(params.time);

        step_wire.draw(ctx);
        ctx.restore();
    });

    // Request/Response Animation
    function add_packet(name, y_offset, color, direction) {
        vid.add_object(name, { opacity: 0, progress: 0 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha = params.opacity;

            let start_x = (direction === 'forward') ? host.right_center().x : mcu.left_center().x;
            let end_x = (direction === 'forward') ? mcu.left_center().x : host.right_center().x;

            let current_x = start_x + (end_x - start_x) * params.progress;
            let current_y = host.right_center().y + y_offset;

            // Draw solid box for packet (filled white, stroked color)
            ctx.fillStyle = '#ccc';
            ctx.strokeStyle = color;
            ctx.lineWidth = 2;
            let packet_w = 40;
            let packet_h = 24;
            ctx.fillRect(current_x - packet_w / 2, current_y - packet_h / 2, packet_w, packet_h);
            ctx.strokeRect(current_x - packet_w / 2, current_y - packet_h / 2, packet_w, packet_h);

            ctx.restore();
        });
    }

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title, wires, and then boxes so boxes clip the wires
    vid.add_transition(['title', 'usb_wire', 'step_wire_pulse'], t, 1.0, { opacity: 1 });
    vid.add_transition(['host_box', 'mcu_box', 'driver_box'], t, 1.0, { opacity: 1 });

    vid.add_transition(['step_wire_pulse'], t, 1.0, { time: t });
    t += 1.0;
    t += pause;

    // 2. Slow rate of commands
    for (let i = 0; i < 3; i++) {
        let req_name = `req_${i}`;
        let res_name = `res_${i}`;

        add_packet(req_name, -20, '#000', 'forward');
        add_packet(res_name, 20, '#000', 'reverse');

        vid.add_transition([req_name], t, 0.1, { opacity: 1 });
        vid.add_transition([req_name], t, 1.0, { progress: 1 });
        vid.add_transition([req_name], t + 1.0, 0.1, { opacity: 0 });

        t += 1.2;

        vid.add_transition([res_name], t, 0.1, { opacity: 1 });
        vid.add_transition([res_name], t, 1.0, { progress: 1 });
        vid.add_transition([res_name], t + 1.0, 0.1, { opacity: 0 });

        t += 1.2;

        // Progress time on wire
        vid.add_transition(['step_wire_pulse'], t - 2.4, 2.4, { time: t });
    }

    t += pause;
    vid.add_transition(['step_wire_pulse'], t - pause, pause, { time: t });


    // 3. Wires and Initial slow step pulse
    let slow_pulses = 6;
    let slow_pulse_period = 0.5;

    let current_val = 0;

    for (let i = 0; i < slow_pulses; i++) {
        current_val = 1 - current_val; // Flip 0 -> 1 or 1 -> 0
        step_graph.flip(t, current_val);
        vid.add_transition(['step_wire_pulse'], t, slow_pulse_period, { time: t + slow_pulse_period });
        t += slow_pulse_period;
    }

    t += pause;
    vid.add_transition(['step_wire_pulse'], t - pause, pause, { time: t });

    // 4. Increase frequency (decrease gap)
    let fast_pulses = 30;
    let min_period = 0.05;
    let current_period = slow_pulse_period;
    let start_peak_t = 0;
    let peak_t = 0;

    for (let i = 0; i < fast_pulses; i++) {
        current_val = 1 - current_val;
        step_graph.flip(t, current_val);

        // Progressively decrease gap
        current_period = Math.max(min_period, current_period * 0.5);

        if (i === fast_pulses - 2) {
            start_peak_t = t;
        }
        if (i === fast_pulses - 1) {
            peak_t = t; // Save time of peak velocity pulse end
        }

        vid.add_transition(['step_wire_pulse'], t, current_period, { time: t + current_period });
        t += current_period;
    }

    // Highlight the peak velocity pulse when frequency is highest
    vid.add_object('peak_highlight', { opacity: 0, current_time: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha = params.opacity;

        let start = mcu.right_center();
        let end = driver.left_center();
        let width = end.x - start.x;
        let y = start.y;

        // Calculate x position based on propagation delay logic from WireGraph
        let prop_delay = 0.5;
        let x_norm_start = (start_peak_t - params.current_time) / -prop_delay;
        let x_norm_end = (peak_t - params.current_time) / -prop_delay;

        if (x_norm_start >= 0 && x_norm_end <= 1) {
            let px_start = start.x + x_norm_start * width;
            let px_end = start.x + x_norm_end * width;

            ctx.strokeStyle = 'red';
            ctx.setLineDash([5, 5]);
            ctx.lineWidth = 2;

            // Draw two vertical dashed lines extending down
            ctx.beginPath();
            ctx.moveTo(px_start, y + 35);
            ctx.lineTo(px_start, y + 80);

            ctx.moveTo(px_end, y + 35);
            ctx.lineTo(px_end, y + 80);

            ctx.stroke();

            // Arrows at ends of horizontal line
            ctx.fillStyle = 'red';
            ctx.textAlign = 'center';
            ctx.font = '20px "Noto Sans Mono"';
            let mid_x = (px_start + px_end) / 2;
            ctx.fillText("Peak Velocity", mid_x + 60, y + 105);
        }

        ctx.restore();
    });

    // Pause step pulses and highlight peak
    // Stop time progression to freeze wave
    vid.add_transition(['peak_highlight'], 0, t, { current_time: t });
    vid.add_transition(['peak_highlight'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['peak_highlight'], t, 0.5, { opacity: 0 });
    // vid.add_transition(['step_wire_pulse'], t, 0.5, { opacity: 0, time: t }); // Fade out the wire
    t += 0.5;

    // 5. Flood of requests/responses for Load Testing
    let num_flood = 20;
    for (let i = 0; i < num_flood; i++) {
        let name_req = `flood_req_${i}`;
        let name_res = `flood_res_${i}`;
        add_packet(name_req, -20, '#000', 'forward');
        add_packet(name_res, 20, '#000', 'reverse');

        // Overlap them
        let f_t = t + i * 0.15;

        vid.add_transition([name_req], f_t, 0.1, { opacity: 1 });
        vid.add_transition([name_req], f_t, 0.5, { progress: 1 });
        vid.add_transition([name_req], f_t + 0.5, 0.1, { opacity: 0 });

        vid.add_transition([name_res], f_t + 0.2, 0.1, { opacity: 1 });
        vid.add_transition([name_res], f_t + 0.2, 0.5, { progress: 1 });
        vid.add_transition([name_res], f_t + 0.7, 0.1, { opacity: 0 });
    }

    t += num_flood * 0.15 + 1.0;
    t += pause;

    vid.move_to_top('mcu_box');
    vid.move_to_top('host_box');
    vid.move_to_top('peak_highlight');

    vid.set_duration(t);
    return vid;
}


function part4_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part4");
    vid.add_object('title', { opacity: 0, text: 'Host Processing Cycles' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let grid = slide_body_grid(canvas).split(2, 1);

    /*
    Graph will be 400ms in length.
    */
    let move_pts = [
        // Time 0ms
        // => Nothing for 100ms

        // Time 100ms
        { x: 0.25, y: 0.2 },
        { x: 0.5, y: 0.6 },
        { x: 0.6, y: 0.2 },
        { x: 0.75, y: 0.4 },
        { x: 0.8, y: 0.7 },
        { x: 0.85, y: 0.6 },
        { x: 0.9, y: 0.75 },
        { x: 0.95, y: 0.2 },
        { x: 1, y: 0.4 },
    ];

    let cell1 = grid.cell(0, 0);
    let pos1 = cell1.bottom_left();

    let mcu_cell = grid.cell(1.2, 0);
    let mcu_box = new DiagramBox({
        text: 'Microcontroller',
        width: 360,
        height: 140,
        font_size: 20,
        position: mcu_cell.center(),
        text_offset: { x: 0, y: -50 }
    });

    let red_xs = [0.25, 0.25 + 0.25 / 3, 0.25 + 0.5 / 3, 0.5];
    let green_xs = [0.5, 0.55, 0.6, 0.65, 0.7, 0.75];
    let purple_xs = [];
    for (let i = 0; i < 10; i++) {
        purple_xs.push(0.75 + i * (0.25 / 9));
    }

    let motor_pos = { x: mcu_box.position().x + 350, y: mcu_box.position().y };
    let motor_box = new DiagramBox({
        text: 'Motor',
        width: 140,
        height: 140,
        font_size: 20,
        position: motor_pos,
        text_offset: { x: 0, y: -50 }
    });

    let get_graph_pt = (x, y) => {
        x = pos1.x + x * cell1.width();
        y = pos1.y - y * (cell1.height() - 20)

        x += 3;
        y -= 3;

        return { x, y }
    };

    vid.add_object('graph', { opacity: 0, max: 0 }, (ctx, params) => {
        let ft = (t) => {
            if (t > params.max) {
                return null;
            }

            let x = move_pts[0].x + t * (move_pts[move_pts.length - 1].x - move_pts[0].x);
            let y = getInterpolatedY(move_pts, x);

            return { x, y };
        }

        ctx.save();
        ctx.translate(pos1.x, pos1.y);

        draw_graph(ctx, {
            width: cell1.width(),
            height: cell1.height() - 20,
            y_label: 'Position',
            x_label: 'Time',
            color: '#0bf',
            font_size: 20
        }, ft);
        ctx.restore();

        // Draw axes lines (independent of max clipping)
        let lines = [
            { x: 0.0, text: '0ms' },
            { x: 0.25, text: '100ms' },
            { x: 0.5, text: '200ms' },
            { x: 0.75, text: '300ms' },
        ]

        lines.map((line) => {
            ctx.save();

            let start = get_graph_pt(line.x, 0);
            let end = get_graph_pt(line.x, 1);

            if (line.x != 0.0) {
                ctx.strokeStyle = '#888';
                ctx.lineWidth = 2;
                ctx.setLineDash([5, 5]);
                ctx.beginPath();
                ctx.moveTo(start.x, start.y);
                ctx.lineTo(end.x, end.y);
                ctx.stroke();
            }

            ctx.translate(
                start.x,
                start.y + 10
            );
            draw_multiline_text(ctx, {
                text: line.text,
                font_size: 16,
                text_baseline: 'top',
                font_family: "Noto Sans Mono",
                text_align: 'center',
                color: '#444'
            });

            ctx.restore();

        });
    });

    vid.add_object('now', { opacity: 0, time_x: 0.0 }, (ctx, params) => {
        let now_pt = get_graph_pt(params.time_x, 0.0);
        now_pt.y += 40;

        ctx.fillStyle = 'red';

        ctx.beginPath();
        let size = 10;
        ctx.moveTo(now_pt.x, now_pt.y);
        ctx.lineTo(now_pt.x + size, now_pt.y + size);
        ctx.lineTo(now_pt.x - size, now_pt.y + size);
        ctx.closePath();
        ctx.fill();

        ctx.translate(
            now_pt.x,
            now_pt.y + 20
        );
        draw_multiline_text(ctx, {
            text: 'Now',
            font_size: 16,
            text_baseline: 'top',
            font_family: "Noto Sans Mono",
            text_align: 'center',
            color: 'red'
        });

    });

    vid.add_object('mcu', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        mcu_box.draw(ctx);
        ctx.restore();
    });

    vid.add_object('motion_queue', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;

        let pos = mcu_box.position();
        let boxWidth = 20;
        let boxHeight = 20;
        let numBoxes = 10;
        let startX = pos.x - (numBoxes * boxWidth) / 2;
        let startY = pos.y;

        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;

        for (let i = 0; i < numBoxes; i++) {
            ctx.strokeRect(startX + i * boxWidth, startY, boxWidth, boxHeight);
        }

        ctx.translate(pos.x, startY + boxHeight + 15);
        draw_multiline_text(ctx, {
            text: 'Motion Queue',
            font_size: 16,
            text_baseline: 'top',
            font_family: "Noto Sans Mono",
            text_align: 'center',
            color: '#000'
        });

        ctx.restore();
    });

    vid.add_object('mcu_motor_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        drawArrowPos(
            ctx,
            mcu_box.right_center(),
            motor_box.left_center(),
            2, 20, false
        );
        ctx.restore();
    });

    vid.add_object('motor', { opacity: 0, angle: 0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        motor_box.draw(ctx);

        ctx.translate(motor_pos.x, motor_pos.y);

        // draw shaft circle
        ctx.beginPath();
        ctx.arc(0, 0, 10, 0, Math.PI * 2);
        ctx.fillStyle = '#aaa';
        ctx.fill();
        ctx.stroke();

        ctx.rotate(params.angle);

        // draw bar magnet
        ctx.fillStyle = 'red'; // north
        ctx.fillRect(-5, -40, 10, 40);
        ctx.fillStyle = 'blue'; // south
        ctx.fillRect(-5, 0, 10, 40);
        ctx.strokeRect(-5, -40, 10, 80);

        ctx.restore();
    });

    function add_dot_batch(name, color, point_xs, queue_start_idx) {
        let initial_params = {
            opacity: 1,
            intro_x_progress: 1,
            spawn_progress: 0,
            queue_progress: 0,
            queue_offset: 0,
        };
        for (let i = 0; i < point_xs.length; i++) {
            initial_params['dequeue_progress_' + i] = 0;
        }

        vid.add_object(name, initial_params, (ctx, params) => {
            ctx.save();
            ctx.globalAlpha = params.opacity;

            let boxWidth = 20;
            let boxHeight = 20;
            let numBoxes = 10;
            let mcu_pos = mcu_box.position();
            let queueStartX = mcu_pos.x - (numBoxes * boxWidth) / 2;
            let queueStartY = mcu_pos.y;

            for (let i = 0; i < point_xs.length; i++) {
                let p_x = point_xs[i];
                let p_y = getInterpolatedY(move_pts, p_x);

                let graph_pt = get_graph_pt(p_x, p_y);

                // Bucket destination
                let bucket_x = queueStartX + (queue_start_idx + i - params.queue_offset) * boxWidth + boxWidth / 2;
                let bucket_y = queueStartY + boxHeight / 2;

                let motor_dest_x = motor_box.position().x;
                let motor_dest_y = motor_box.position().y;

                let cur_x = graph_pt.x;
                let cur_y = graph_pt.y;

                // Transition: intro_x_progress (mostly for first dot)
                if (params.intro_x_progress < 1) {
                    let start_p_x = 0;
                    let start_p_y = getInterpolatedY(move_pts, start_p_x);
                    let initial_px = get_graph_pt(start_p_x, start_p_y);

                    cur_x = initial_px.x + (graph_pt.x - initial_px.x) * params.intro_x_progress;
                    cur_y = initial_px.y + (graph_pt.y - initial_px.y) * params.intro_x_progress;
                }

                // Transition: graph -> queue
                if (params.queue_progress > 0) {
                    cur_x = graph_pt.x + (bucket_x - graph_pt.x) * params.queue_progress;
                    cur_y = graph_pt.y + (bucket_y - graph_pt.y) * params.queue_progress;
                }

                // Transition: queue -> motor
                let deq_p = params['dequeue_progress_' + i];
                let dot_opacity = getObjectAlpha(i, point_xs.length, params.spawn_progress, 0.5);

                if (i === 0 && params.intro_x_progress < 1) {
                    dot_opacity = 1;
                }

                if (deq_p > 0) {
                    if (deq_p < 0.2) {
                        let p = deq_p / 0.2;
                        cur_x = bucket_x;
                        cur_y = bucket_y + 50 * p;
                    } else if (deq_p < 0.8) {
                        let p = (deq_p - 0.2) / 0.6;
                        cur_x = bucket_x + (motor_dest_x - bucket_x) * p;
                        cur_y = bucket_y + 50;
                    } else {
                        let p = (deq_p - 0.8) / 0.2;
                        cur_x = motor_dest_x;
                        cur_y = bucket_y + 50 + (motor_dest_y - (bucket_y + 50)) * p;
                        dot_opacity = (1 - p) * dot_opacity;
                    }
                }

                if (dot_opacity > 0) {
                    ctx.globalAlpha = params.opacity * dot_opacity;
                    ctx.beginPath();
                    ctx.arc(cur_x, cur_y, 6, 0, Math.PI * 2);
                    ctx.fillStyle = color;
                    ctx.fill();
                }
            }
            ctx.restore();
        });
    }

    add_dot_batch('red_dots', 'red', red_xs, 0);
    add_dot_batch('green_dots', '#0a0', green_xs, 4);
    add_dot_batch('purple_dots', 'purple', purple_xs, 10);

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'graph', 'now'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    // Slide in graph data
    vid.add_transition(['graph'], t, 1.0, { max: 1 });
    t += 1.0;
    t += pause;

    // Set first dot to start at x=0
    vid.add_transition(['red_dots'], t, 0, { intro_x_progress: 0, spawn_progress: 0 });
    vid.add_transition(['red_dots'], t, 1.0, { intro_x_progress: 1, spawn_progress: 1 });
    vid.add_transition(['now'], t, 1.0, { time_x: 0.05 });
    t += 1.0;
    t += pause;

    // Fade in MCU and related objects
    vid.add_transition(['mcu', 'motion_queue', 'motor', 'mcu_motor_arrow'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['red_dots'], t, 1.0, { queue_progress: 1 });
    vid.add_transition(['now'], t, 1.0, { time_x: 0.1 });
    t += 1.0;
    t += pause;

    vid.add_transition(['now'], t, 3.0, { time_x: 0.25 });
    t += 3.0;

    let total_motor_steps = 0;

    let sweep_start_t = t;
    vid.add_transition(['now'], t, 5.0, { time_x: 0.5 });
    vid.add_transition(['green_dots'], sweep_start_t, 2.0, { spawn_progress: 1 });
    vid.add_transition(['green_dots'], sweep_start_t + 2.5, 2.0, { queue_progress: 1 });

    for (let i = 0; i < 4; i++) {
        let p_rel = (red_xs[i] - 0.25) / 0.25;
        let launch_t = sweep_start_t + p_rel * 5.0;

        let deq_param = {};
        deq_param['dequeue_progress_' + i] = 1;
        deq_param['queue_offset'] = i + 1;

        vid.add_transition(['red_dots'], launch_t, 1.0, deq_param);
        vid.add_transition(['green_dots', 'purple_dots'], launch_t, 1.0, { queue_offset: i + 1 });
        vid.add_transition(['motor'], launch_t + 0.8, 0.2, { angle: deg2rad((total_motor_steps + 1) * 45) });
        total_motor_steps++;
    }
    t += 5.0;
    t += pause;

    let sweep2_start_t = t;
    vid.add_transition(['now'], t, 5.0, { time_x: 0.75 });
    vid.add_transition(['purple_dots'], sweep2_start_t, 2.0, { spawn_progress: 1 });
    vid.add_transition(['purple_dots'], sweep2_start_t + 2.5, 2.0, { queue_progress: 1 });

    for (let i = 0; i < 6; i++) {
        let p_rel = (green_xs[i] - 0.5) / 0.25;
        let launch_t = sweep2_start_t + p_rel * 5.0;

        let deq_param = {};
        deq_param['dequeue_progress_' + i] = 1;
        deq_param['queue_offset'] = 4 + i + 1;

        vid.add_transition(['green_dots'], launch_t, 1.0, deq_param);
        vid.add_transition(['purple_dots'], launch_t, 1.0, { queue_offset: 4 + i + 1 });
        vid.add_transition(['motor'], launch_t + 0.8, 0.2, { angle: deg2rad((total_motor_steps + 1) * 45) });
        total_motor_steps++;
    }
    t += 5.0;
    t += pause;

    let sweep3_start_t = t;
    vid.add_transition(['now'], t, 5.0, { time_x: 1.0 });

    for (let i = 0; i < 10; i++) {
        let p_rel = (purple_xs[i] - 0.75) / 0.25;
        let launch_t = sweep3_start_t + p_rel * 5.0;

        let deq_param = {};
        deq_param['dequeue_progress_' + i] = 1;
        deq_param['queue_offset'] = 10 + i + 1;

        vid.add_transition(['purple_dots'], launch_t, 1.0, deq_param);
        vid.add_transition(['motor'], launch_t + 0.8, 0.2, { angle: deg2rad((total_motor_steps + 1) * 45) });
        total_motor_steps++;
    }
    t += 5.0;
    t += pause;

    vid.set_duration(t);

    return vid;

}

function part3_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part3");
    vid.add_object('title', { opacity: 0, text: 'Time Estimation' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let grid = slide_body_grid(canvas).split(2, 5);

    let new_color = '#084';
    let new_color_bg = hexToRgba(new_color, 0.3);

    let slicer_cell = grid.cell(0, 2);
    let slicer_box = new DiagramBox({
        text: 'Slicer\'s Time Estimator',
        width: 400,
        height: 150,
        font_size: 20,
        text_offset: { x: 0, y: -50 },
        position: slicer_cell.center()
    });

    let firmware_cell = grid.cell(1, 2);
    let firmware_box = new DiagramBox({
        text: 'Printer\'s Firmware',
        width: 400,
        height: 150,
        font_size: 20,
        text_offset: { x: 0, y: -50 },
        position: firmware_cell.center()
    });

    let slicer_settings_cell = grid.cell(0, 0);
    let slicer_settings = new DiagramBox({
        text: 'Slicer Settings',
        width: 160,
        height: 80,
        font_size: 20,
        position: slicer_settings_cell.center()
    });


    let clock_cell = grid.cell(0, 1.4);
    let clock_box = new DiagramBox({
        text: 'Real Time\nClock',
        width: 160,
        height: 80,
        font_size: 20,
        position: clock_cell.center()
    });
    let fake_clock_box = new DiagramBox({
        text: 'Fake\nClock',
        width: 160,
        height: 80,
        font_size: 20,
        position: clock_cell.center(),
        background_color: new_color_bg,
        stroke_color: new_color
    });

    let heater_cell = grid.cell(0, 2.4);
    let heater_box = new DiagramBox({
        text: 'Heater\nSimulation',
        width: 160,
        height: 80,
        font_size: 20,
        position: heater_cell.center(),
        background_color: new_color_bg,
        stroke_color: new_color
    });

    let gcode_cell = grid.cell(0.5, 0);
    let gcode_box = new DiagramBox({
        text: 'GCode File',
        width: 160,
        height: 80,
        font_size: 20,
        position: gcode_cell.center()
    });

    let printer_settings_cell = grid.cell(1, 0);
    let printer_settings_box = new DiagramBox({
        text: 'Printer Settings',
        width: 160,
        height: 80,
        font_size: 20,
        position: printer_settings_cell.center()
    });

    let estimate_cell = grid.cell(0, 4);
    let estimate_box = new DiagramBox({
        text: 'Time Estimate\n(e.g. 1.5 hours)',
        width: 160,
        height: 80,
        font_size: 18,
        position: estimate_cell.center()
    });

    let log_cell = grid.cell(1, 4);
    let log_box = new DiagramBox({
        text: 'Log File',
        width: 200,
        height: 160,
        font_size: 16,
        text_offset: { x: -60, y: -60 },
        position: log_cell.center(),
        stroke_color: new_color,
        background_color: new_color_bg
    });

    let step_times_cell = grid.cell(1, 4);
    let step_times_box = new DiagramBox({
        text: 'Step Times\n(0s, 0.1s, 0.2s, ...)',
        width: 160,
        height: 80,
        font_size: 18,
        position: step_times_cell.center()
    });

    let slicer_inner_texts = [
        'Acceleration\nCurves',
        'Cornering\nSpeeds',
        'Machine\nLimits'
    ];

    let slicer_inner_boxes = [];

    for (var i = 0; i < slicer_inner_texts.length; i++) {
        let cell = grid.cell(0, 2 + (i - 1) * 0.7);

        slicer_inner_boxes.push(new DiagramBox({
            text: slicer_inner_texts[i],
            width: 100,
            height: 60,
            font_size: 16,
            position: cell.center()
        }));
    }

    let firmware_inner_texts = [
        'Accel.\nCurves',
        'Cornering\nSpeeds',
        'Input\nShaping',
        'Printer\nGeometry',
        'Step\nInterp.'
    ];

    let firmware_inner_boxes = [];

    for (var i = 0; i < firmware_inner_texts.length; i++) {
        let cell = grid.cell(1, 2 + (i - 2) * 0.45);

        firmware_inner_boxes.push(new DiagramBox({
            text: firmware_inner_texts[i],
            width: 60,
            height: 60,
            font_size: 12,
            position: cell.center()
        }));
    }



    vid.add_object('slicer_box', { opacity: 0 }, (ctx, params) => {
        slicer_box.draw(ctx);
    });
    vid.add_object('slicer_settings', { opacity: 0 }, (ctx, params) => {
        slicer_settings.draw(ctx);

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            slicer_settings.right_center(),
            slicer_box.left_center(),
            2, 20, false
        );

    });
    vid.add_object('gcode_box', { opacity: 0 }, (ctx, params) => {
        gcode_box.draw(ctx);

    });

    vid.add_object('gcode_arrow', { opacity: 0 }, (ctx, params) => {
        let end = slicer_box.left_center();
        end.y += 40;

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            gcode_box.right_center(),
            end,
            2, 20, false
        );
    });


    vid.add_object('printer_settings_box', { opacity: 0 }, (ctx, params) => {
        printer_settings_box.draw(ctx);

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            printer_settings_box.right_center(),
            firmware_box.left_center(),
            2, 20, false
        );

    });
    vid.add_object('firmware_box', { opacity: 0 }, (ctx, params) => {
        firmware_box.draw(ctx);

        let end = firmware_box.left_center();
        end.y -= 40;

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            gcode_box.right_center(),
            end,
            2, 20, false
        );

    });

    vid.add_object('estimate_box', { opacity: 0 }, (ctx, params) => {
        estimate_box.draw(ctx);
    });

    // log_box
    vid.add_object('log_box', { opacity: 0 }, (ctx, params) => {
        log_box.draw(ctx);
    });


    vid.add_object('step_times_box', { opacity: 0 }, (ctx, params) => {
        step_times_box.draw(ctx);
    });

    vid.add_object('slicer_inner', { opacity: 0 }, (ctx, params) => {
        slicer_inner_boxes.map((b, i) => {
            b.draw(ctx);

            if (i + 1 < slicer_inner_boxes.length) {
                ctx.fillStyle = '#000';
                drawArrowPos(
                    ctx,
                    b.right_center(),
                    slicer_inner_boxes[i + 1].left_center(),
                    2, 10, false
                );
            }
        });

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            slicer_box.right_center(),
            estimate_box.left_center(),
            2, 20, false
        );
    });

    vid.add_object('firmware_inner', { opacity: 0 }, (ctx, params) => {
        firmware_inner_boxes.map((b, i) => {
            b.draw(ctx);

            if (i + 1 < firmware_inner_boxes.length) {
                ctx.fillStyle = '#000';
                drawArrowPos(
                    ctx,
                    b.right_center(),
                    firmware_inner_boxes[i + 1].left_center(),
                    2, 10, false
                );
            }
        });

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            firmware_box.right_center(),
            step_times_box.left_center(),
            2, 20, false
        );
    });

    vid.add_object('new_stuff_text', { opacity: 0 }, (ctx, params) => {
        ctx.translate(
            940,
            520
        );
        draw_multiline_text(ctx, {
            text: `* Green = New Stuff`,
            font_size: 20,
            font_family: "Noto Sans Mono",
            text_align: 'right',
            color: new_color
        });
    });

    vid.add_object('new_estimate_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = new_color;
        ctx.strokeStyle = new_color;
        drawArrowPos(
            ctx,
            step_times_box.top_center(),
            estimate_box.bottom_center(),
            2, 20, false
        );

    });

    vid.add_object('clock_box', { opacity: 0 }, (ctx, params) => {
        clock_box.draw(ctx);

        let end = firmware_box.top_center();
        end.x = clock_box.bottom_center().x;

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            clock_box.bottom_center(),
            end,
            2, 20, false
        );

    });

    vid.add_object('fake_clock_box', { opacity: 0 }, (ctx, params) => {
        fake_clock_box.draw(ctx);

        let end = firmware_box.top_center();
        end.x = clock_box.bottom_center().x;

        ctx.fillStyle = new_color;
        ctx.strokeStyle = new_color;
        drawArrowPos(
            ctx,
            clock_box.bottom_center(),
            end,
            2, 20, false
        );
    });

    vid.add_object('heater_box', { opacity: 0 }, (ctx, params) => {
        heater_box.draw(ctx);

        let end = firmware_box.top_center();
        end.x = heater_box.bottom_center().x;


        ctx.fillStyle = new_color;
        ctx.strokeStyle = new_color;
        drawArrowPos(
            ctx,
            heater_box.bottom_center(),
            end,
            2, 20, true
        );
    });

    vid.add_object('settings_highlight', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = 'rgba(0,0,0,0)';
        ctx.strokeStyle = 'red';
        ctx.lineWidth = 4;

        let margin = 20;

        {
            ctx.save();
            ctx.translate(slicer_settings.position().x, slicer_settings.position().y);
            draw_box(ctx, slicer_settings._width + margin, slicer_settings._height + margin);

            ctx.translate(0, -65);
            draw_multiline_text(ctx, {
                text: 'Not Synced',
                font_size: 20,
                text_align: 'center',
                color: 'red'
            });

            ctx.restore();
        }

        {
            ctx.save();
            ctx.translate(printer_settings_box.position().x, printer_settings_box.position().y);
            draw_box(ctx, printer_settings_box._width + margin, printer_settings_box._height + margin);

            ctx.translate(0, 65);
            draw_multiline_text(ctx, {
                text: 'Not Synced',
                font_size: 20,
                text_align: 'center',
                color: 'red'
            });

            ctx.restore();
        }
    });

    vid.add_object('code_highlight', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = 'rgba(0,0,0,0)';
        ctx.strokeStyle = 'red';
        ctx.lineWidth = 4;

        let margin = 20;

        {
            ctx.save();
            ctx.translate(slicer_box.position().x, slicer_box.position().y);
            draw_box(ctx, slicer_box._width + margin, slicer_box._height - 50);

            ctx.translate(0, -100);
            draw_multiline_text(ctx, {
                text: 'Rewritten code for the Slicer',
                font_size: 20,
                text_align: 'center',
                color: 'red'
            });

            ctx.restore();
        }

        {
            ctx.save();
            ctx.translate(firmware_box.position().x, firmware_box.position().y);
            draw_box(ctx, firmware_box._width + margin, firmware_box._height - 50);

            ctx.translate(0, 100);
            draw_multiline_text(ctx, {
                text: 'Different code for every firmware',
                font_size: 20,
                text_align: 'center',
                color: 'red'
            });

            ctx.restore();
        }


    });


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'slicer_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['slicer_settings'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['gcode_box', 'gcode_arrow'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['slicer_inner'], t, 0.5, { opacity: 1 });
    t += 0.5;
    vid.add_transition(['estimate_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['firmware_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['printer_settings_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['firmware_inner'], t, 0.5, { opacity: 1 });
    t += 0.5;
    vid.add_transition(['step_times_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['settings_highlight'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['settings_highlight'], t, 0.5, { opacity: 0 });
    vid.add_transition(['code_highlight'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['code_highlight'], t, 0.5, { opacity: 0 });
    t += 0.5;
    t += pause;

    vid.add_transition(['slicer_box', 'slicer_settings', 'slicer_inner', 'gcode_arrow'], t, 0.5, { opacity: 0 });
    t += 0.5;
    vid.add_transition(['new_estimate_arrow', 'new_stuff_text'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;


    vid.add_transition(['clock_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['clock_box'], t, 0.5, { opacity: 0 });
    vid.add_transition(['fake_clock_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['log_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['heater_box'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;


    vid.set_duration(t);

    return vid;
}