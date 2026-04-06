import { Timeline, draw_title, deg2rad, draw_box, slide_body_grid, DiagramBox, WireBundle, Wire, shallow_copy, draw_multiline_text, draw_box_text } from '../../utils.js';
import { hexToRgba } from '../../hex_to_rgba.js';
import { drawArrow, drawArrowPos } from '../../arrow.js';
import { getPointAtY } from '../../y_point.js';
import { drawPolyline, drawSequentialChains, drawShearedSquare } from '../../sheared_square.js';
// import { math_to_img, math_scale } from '../../mathjax.js';
import { drawCenteredTable } from '../../centered_table.js';
import { draw_graph } from '../3d_printer/motion_animation.js';
import { getInterpolatedY, interpolateValue } from '../../linear_interp.js';
import { getObjectAlpha } from '../../staggered_fade.js';

export async function configure(canvas) {
    return part11_eink_video(canvas);
    // return part6_key_exchange_video(canvas);
    // return part6_video(canvas);
    // return part6_ack_video(canvas);
    // return part5_video(canvas);
    // return part4_button_video(canvas);
    // return part4_led_video(canvas);
    // return part2_video(canvas);
}

function part4_led_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part4_led_video");

    vid.add_object('title', { opacity: 0, text: 'LED Blink Code' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 4);

    let width = 180;
    let height = 90;
    let font_size = 22;

    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 320;

    let setup_box = new DiagramBox({
        text: 'Setup 1s\nTimer',
        width,
        height,
        font_size,
        position: { x: center_x - spacing, y: center_y },
    });
    vid.add_object('setup_box', { opacity: 0 }, (ctx, params) => {
        setup_box.draw(ctx);
    });

    let wfi_box = new DiagramBox({
        text: 'WFI\n(Wait for Interrupt)',
        width,
        height,
        font_size: 18,
        position: { x: center_x, y: center_y },
    });
    vid.add_object('wfi_box', { opacity: 0 }, (ctx, params) => {
        wfi_box.draw(ctx);
    });

    let flip_box = new DiagramBox({
        text: 'Flip LED\nOn/Off',
        width,
        height,
        font_size,
        position: { x: center_x + spacing, y: center_y },
    });
    vid.add_object('flip_box', { opacity: 0 }, (ctx, params) => {
        flip_box.draw(ctx);
    });

    vid.add_object('setup_to_wfi_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, setup_box.right_center(), wfi_box.left_center(), 2, 20, false);
    });

    vid.add_object('wfi_to_flip_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, wfi_box.right_center(), flip_box.left_center(), 2, 20, false);
    });

    vid.add_object('flip_to_setup_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;

        let start = flip_box.bottom_center();
        let end = setup_box.bottom_center();

        ctx.beginPath();
        ctx.moveTo(start.x, start.y);
        ctx.lineTo(start.x, start.y + 40);
        ctx.lineTo(end.x, start.y + 40);
        ctx.stroke();

        let head_start = { x: end.x, y: start.y + 40 };
        drawArrowPos(ctx, head_start, end, 2, 20, false);

        ctx.restore();
    });

    let timer_box = new DiagramBox({
        text: 'Timer',
        width,
        height,
        font_size,
        position: { x: center_x - spacing, y: body_grid.split(3, 1).cell(0, 0).center().y },
    });
    vid.add_object('timer_box', { opacity: 0, progress: 1.0 }, (ctx, params) => {
        ctx.save();
        ctx.translate(timer_box.position().x, timer_box.position().y);
        ctx.fillStyle = '#aaccee';
        ctx.strokeStyle = '#000';
        draw_box(ctx, width, height);

        ctx.fillStyle = '#000';
        ctx.font = `${font_size}px "Noto Sans"`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('Timer', -30, 0);

        let x_offset = 40;
        let radius = 20;
        ctx.beginPath();
        ctx.arc(x_offset, 0, radius, -Math.PI / 2, -Math.PI / 2 + params.progress * 2 * Math.PI, false);
        ctx.lineTo(x_offset, 0);
        ctx.fillStyle = '#003399';
        ctx.fill();

        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.beginPath();
        ctx.arc(x_offset, 0, radius, 0, 2 * Math.PI);
        ctx.stroke();

        ctx.restore();
    });

    vid.add_object('wfi_outline', { opacity: 0 }, (ctx, params) => {
        let pos = wfi_box.position();
        let outline_width = width + 20;
        let outline_height = height + 20;

        let box = new DiagramBox({
            text: 'Sleeping / Low Power State',
            font_size: 18,
            text_color: '#f00',
            background_color: 'rgba(0,0,0,0)',
            width: outline_width,
            height: outline_height,
            position: pos,
            stroke_color: '#f00',
            text_offset: { x: 0, y: (outline_height / 2) + 20 }
        });

        ctx.lineWidth = 4;
        ctx.setLineDash([5, 5]);
        box.draw(ctx);
    });

    vid.add_object('timer_to_wfi_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#000';
        let start = { x: timer_box.position().x + width / 2, y: timer_box.position().y + height / 2 };
        let end = { x: wfi_box.position().x - width / 2, y: wfi_box.position().y - height / 2 };
        drawArrowPos(ctx, start, end, 2, 20, false);
    });

    let pause = 0.5;
    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['setup_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['timer_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['wfi_box', 'setup_to_wfi_arrow'], t, 0.5, { opacity: 1 });
    vid.add_transition(['setup_box'], t, 0.5, { opacity: 0.5 });
    t += 0.5 + pause;

    vid.add_transition(['wfi_outline'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['wfi_outline'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    vid.add_transition(['timer_box'], t, 2.0, { progress: 0.0 });
    t += 2.0;

    vid.add_transition(['timer_to_wfi_arrow'], t, 0.3, { opacity: 1 });
    t += 0.3 + pause;

    vid.add_transition(['timer_box', 'timer_to_wfi_arrow'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    vid.add_transition(['flip_box', 'wfi_to_flip_arrow'], t, 0.5, { opacity: 1 });
    vid.add_transition(['wfi_box', 'setup_to_wfi_arrow'], t, 0.5, { opacity: 0.5 });
    t += 0.5 + pause;

    vid.add_transition(['flip_to_setup_arrow'], t, 0.5, { opacity: 1 });
    vid.add_transition(['flip_box', 'wfi_to_flip_arrow'], t, 0.5, { opacity: 0.5 });
    vid.add_transition(['setup_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.set_duration(t + 1);

    return vid;
}

function part4_button_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part4_button_video");

    vid.add_object('title', { opacity: 0, text: 'Button Press Code' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 4);

    let width = 180;
    let height = 90;
    let font_size = 22;

    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 320;

    let setup_box = new DiagramBox({
        text: 'Setup Button\nWaiter',
        width,
        height,
        font_size,
        position: { x: center_x - spacing, y: center_y },
    });
    vid.add_object('setup_box', { opacity: 0 }, (ctx, params) => {
        setup_box.draw(ctx);
    });

    let wfi_box = new DiagramBox({
        text: 'WFI\n(Wait for Interrupt)',
        width,
        height,
        font_size: 18,
        position: { x: center_x, y: center_y },
    });
    vid.add_object('wfi_box', { opacity: 0 }, (ctx, params) => {
        wfi_box.draw(ctx);
    });

    let send_box = new DiagramBox({
        text: 'Send\nNotification',
        width,
        height,
        font_size,
        position: { x: center_x + spacing, y: center_y },
    });
    vid.add_object('send_box', { opacity: 0 }, (ctx, params) => {
        send_box.draw(ctx);
    });

    vid.add_object('setup_to_wfi_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, setup_box.right_center(), wfi_box.left_center(), 2, 20, false);
    });

    vid.add_object('wfi_to_send_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, wfi_box.right_center(), send_box.left_center(), 2, 20, false);
    });

    vid.add_object('send_to_setup_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;

        let start = send_box.bottom_center();
        let end = setup_box.bottom_center();

        ctx.beginPath();
        ctx.moveTo(start.x, start.y);
        ctx.lineTo(start.x, start.y + 40);
        ctx.lineTo(end.x, start.y + 40);
        ctx.stroke();

        let head_start = { x: end.x, y: start.y + 40 };
        drawArrowPos(ctx, head_start, end, 2, 20, false);

        ctx.restore();
    });

    let waiter_box = new DiagramBox({
        text: 'Button\nWaiter',
        width,
        height,
        font_size,
        position: { x: center_x - spacing, y: body_grid.split(3, 1).cell(0, 0).center().y },
    });
    vid.add_object('waiter_box', { opacity: 0, progress: 0.0 }, (ctx, params) => {
        ctx.save();
        ctx.translate(waiter_box.position().x, waiter_box.position().y);
        ctx.fillStyle = '#aaccee';
        ctx.strokeStyle = '#000';
        draw_box(ctx, width, height);

        ctx.fillStyle = '#000';
        ctx.font = `${font_size}px "Noto Sans"`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('Button', -30, -12);
        ctx.fillText('Waiter', -30, 12);

        let x_offset = 40;
        let radius = 20;

        ctx.lineWidth = 6;
        ctx.strokeStyle = '#003399';
        ctx.beginPath();
        let angle = params.progress * Math.PI * 2;
        ctx.arc(x_offset, 0, radius, angle, angle + (Math.PI * 1.5), false);
        ctx.stroke();

        ctx.restore();
    });

    vid.add_object('wfi_outline', { opacity: 0 }, (ctx, params) => {
        let pos = wfi_box.position();
        let outline_width = width + 20;
        let outline_height = height + 20;

        let box = new DiagramBox({
            text: 'Sleeping / Low Power State',
            font_size: 18,
            text_color: '#f00',
            background_color: 'rgba(0,0,0,0)',
            width: outline_width,
            height: outline_height,
            position: pos,
            stroke_color: '#f00',
            text_offset: { x: 0, y: (outline_height / 2) + 20 }
        });

        ctx.lineWidth = 4;
        ctx.setLineDash([5, 5]);
        box.draw(ctx);
    });

    vid.add_object('waiter_to_wfi_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.fillStyle = '#000';
        let start = { x: waiter_box.position().x + width / 2, y: waiter_box.position().y + height / 2 };
        let end = { x: wfi_box.position().x - width / 2, y: wfi_box.position().y - height / 2 };
        drawArrowPos(ctx, start, end, 2, 20, false);
    });

    let pause = 0.5;
    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['setup_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['waiter_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['wfi_box', 'setup_to_wfi_arrow'], t, 0.5, { opacity: 1 });
    vid.add_transition(['setup_box'], t, 0.5, { opacity: 0.5 });
    t += 0.5 + pause;

    vid.add_transition(['wfi_outline'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['wfi_outline'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    vid.add_transition(['waiter_box'], t, 2.0, { progress: 4.0 });
    t += 2.0;

    vid.add_transition(['waiter_to_wfi_arrow'], t, 0.3, { opacity: 1 });
    t += 0.3 + pause;

    vid.add_transition(['waiter_box', 'waiter_to_wfi_arrow'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    vid.add_transition(['send_box', 'wfi_to_send_arrow'], t, 0.5, { opacity: 1 });
    vid.add_transition(['wfi_box', 'setup_to_wfi_arrow'], t, 0.5, { opacity: 0.5 });
    t += 0.5 + pause;

    vid.add_transition(['send_to_setup_arrow'], t, 0.5, { opacity: 1 });
    vid.add_transition(['send_box', 'wfi_to_send_arrow'], t, 0.5, { opacity: 0.5 });
    vid.add_transition(['setup_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.set_duration(t + 1);

    return vid;
}

function part5_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part5_video");

    vid.add_object('title', { opacity: 0, text: 'Wireless Protocol' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 350;

    let width = 180;
    let height = 90;
    let font_size = 22;

    let b1_x = center_x - spacing;
    let rx_x = center_x + spacing;

    let b1_y_initial = center_y;
    let b2_y = center_y + 110;

    vid.add_object('button1_box', { opacity: 0, y_offset: 0 }, (ctx, params) => {
        let text = params.y_offset < -50 ? 'Button #1' : 'Button\nBoard';
        let box = new DiagramBox({
            text: text,
            width, height, font_size,
            position: { x: b1_x, y: b1_y_initial + params.y_offset },
        });
        box.draw(ctx);
    });

    vid.add_object('button2_box', { opacity: 0, text_opacity: 1.0 }, (ctx, params) => {
        let box = new DiagramBox({
            text: 'Button #2',
            width, height, font_size,
            position: { x: b1_x, y: b2_y },
            text_color: `rgba(0, 0, 0, ${params.text_opacity})`
        });
        box.draw(ctx);
    });

    vid.add_object('receiver_box', { opacity: 0 }, (ctx, params) => {
        let box = new DiagramBox({
            text: 'Receiver',
            width, height, font_size,
            position: { x: rx_x, y: center_y },
        });
        box.draw(ctx);
    });

    vid.add_object('listen_timer', { opacity: 0, progress: 1.0 }, (ctx, params) => {
        let x_offset = b1_x;
        let y_offset = b2_y;

        ctx.save();
        ctx.translate(x_offset, y_offset);

        let radius = 15;
        ctx.lineWidth = 4;
        ctx.strokeStyle = '#003399';
        ctx.beginPath();
        let angle = params.progress * Math.PI * 2;
        ctx.arc(0, 0, radius, -Math.PI / 2, -Math.PI / 2 + angle, false);
        ctx.stroke();

        ctx.restore();
    });

    function drawPacket(ctx, params, text, x, y) {
        ctx.save();
        ctx.translate(x, y);
        let pwidth = 20 + (text.length * 9);
        let pheight = 36;
        ctx.fillStyle = '#000';
        draw_box(ctx, pwidth, pheight);

        ctx.fillStyle = '#fff';
        ctx.font = `14px "Noto Sans Mono", monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(text, 0, 0);
        ctx.restore();
    }

    function getPath(progress, startX, startY, endX, endY) {
        let px = startX + (endX - startX) * progress;
        let py = startY + (endY - startY) * progress;

        let dist = Math.sqrt((endX - startX) ** 2 + (endY - startY) ** 2);
        if (dist > 0) {
            let nx = -(endY - startY) / dist;
            let ny = (endX - startX) / dist;
            let amplitude = Math.min(15, dist / 15);
            let frequency = Math.PI * 2 * Math.max(1, (dist / 150));
            let wave = Math.sin(progress * frequency) * amplitude;
            px += nx * wave;
            py += ny * wave;
        }
        return { x: px, y: py };
    }

    function addPacketAnim(name, t_start, hold_duration, duration, text_start, text_corrupt, corrupt_progress, startX, startY, endX, endY) {
        vid.add_object(name, { opacity: 0, progress: 0.0, collision_r: 0 }, (ctx, params) => {
            let pos = getPath(params.progress, startX, startY, endX, endY);
            let txt = (params.progress >= corrupt_progress && text_corrupt) ? text_corrupt : text_start;

            if (text_corrupt && params.progress < corrupt_progress && params.progress >= corrupt_progress - 0.3) {
                let coll_pos = getPath(corrupt_progress, startX, startY, endX, endY);
                let p_prog = (params.progress - (corrupt_progress - 0.3)) / 0.3;
                let start_py = coll_pos.y - 300;
                let current_py = start_py + (coll_pos.y - start_py) * p_prog;

                ctx.beginPath();
                ctx.arc(coll_pos.x, current_py, 8, 0, Math.PI * 2);
                ctx.fillStyle = 'rgba(255, 50, 50, 1.0)';
                ctx.fill();
            }

            if (text_corrupt && params.progress >= corrupt_progress && params.collision_r > 0) {
                let coll_pos = getPath(corrupt_progress, startX, startY, endX, endY);
                ctx.beginPath();
                ctx.arc(coll_pos.x, coll_pos.y, params.collision_r, 0, Math.PI * 2);
                ctx.fillStyle = 'rgba(255, 50, 50, 0.7)';
                ctx.fill();
            }

            drawPacket(ctx, params, txt, pos.x, pos.y);
        });

        vid.add_transition([name], t_start, 0.2, { opacity: 1 });
        vid.add_transition([name], t_start + hold_duration, duration, { progress: 1.0 });

        if (text_corrupt) {
            let time_corrupt = t_start + hold_duration + (duration * corrupt_progress);
            vid.add_transition([name], time_corrupt, 0.1, { collision_r: 20 });
            vid.add_transition([name], time_corrupt + 0.1, 0.2, { collision_r: 0 });
        }

        let end_time = t_start + hold_duration + duration;
        vid.add_transition([name], end_time, 0.2, { opacity: 0 });
        return end_time + 0.2;
    }

    function addCollisionAnim(nameA, nameB, t_start, text_startA, text_startB, text_corruptA, text_corruptB, startY_A, startY_B) {
        let duration_to_mid = 1.0;
        let hold_mid = 1.5;
        let duration_to_end = 1.0;
        let dt_start_move = t_start + 0.5;
        let dt_at_mid = dt_start_move + duration_to_mid;
        let dt_leave_mid = dt_at_mid + hold_mid;
        let dt_end = dt_leave_mid + duration_to_end;

        function definePacket(name, startY, text_s, text_c) {
            vid.add_object(name, { opacity: 0, progress: 0.0 }, (ctx, params) => {
                let p = params.progress;
                let px, py;
                let txt = text_s;
                if (p <= 1.0) {
                    px = start_x + (center_x - start_x) * p;
                    py = startY + (center_y - startY) * p;
                } else if (p <= 2.0) {
                    px = center_x;
                    py = center_y;
                    txt = text_c;
                } else {
                    let p2 = (p - 2.0);
                    px = center_x + (end_x - center_x) * p2;
                    py = center_y;
                    txt = text_c;
                }
                if (p >= 1.0) py += (name === nameA ? -8 : 8);
                drawPacket(ctx, params, txt, px, py);
            });

            vid.add_transition([name], t_start, 0.2, { opacity: 1 });
            vid.add_transition([name], dt_start_move, duration_to_mid, { progress: 1.0 });
            vid.add_transition([name], dt_leave_mid, duration_to_end, { progress: 3.0 });
            vid.add_transition([name], dt_end, 0.2, { opacity: 0 });
        }

        definePacket(nameA, startY_A, text_startA, text_corruptA);
        definePacket(nameB, startY_B, text_startB, text_corruptB);

        return dt_end + 0.2;
    }

    let t = 0;
    let pause = 0.5;

    let start_x = b1_x + width / 2;
    let end_x = rx_x - width / 2;
    let current_b1_y = b1_y_initial;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['button1_box', 'receiver_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    t = addPacketAnim('p1', t, 1.5, 2.0, 'Press!', null, 1.0, start_x, current_b1_y, end_x, center_y);
    t += pause;

    t = addPacketAnim('p2', t, 0.5, 2.0, 'Press!', 'P#^s!', 0.5, start_x, current_b1_y, end_x, center_y);
    t += pause;

    t = addPacketAnim('p3', t, 1.5, 2.0, 'Press!Press!', 'Pre$*@ress!', 0.5, start_x, current_b1_y, end_x, center_y);
    t += pause;

    t = addPacketAnim('p4', t, 0.5, 2.0, 'Press!Press!', null, 1.0, start_x, current_b1_y, end_x, center_y);
    t += pause;

    t = addPacketAnim('p5', t, 1.5, 2.0, 'Ack', null, 1.0, end_x, center_y, start_x, current_b1_y);
    t += pause;

    t = addPacketAnim('p6', t, 1.5, 2.0, 'Press! | CRC', null, 1.0, start_x, current_b1_y, end_x, center_y);
    t += pause;

    vid.add_transition(['button1_box'], t, 0.5, { y_offset: -110 });
    current_b1_y -= 110;
    vid.add_transition(['button2_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    t = addPacketAnim('p7', t, 1.5, 2.0, '#1 | Press! | CRC', null, 1.0, start_x, current_b1_y, end_x, center_y);
    t += pause;

    t = addPacketAnim('p8', t, 1.5, 2.0, '#2 | Press! | CRC', null, 1.0, start_x, b2_y, end_x, center_y);
    t += pause;

    t = addCollisionAnim('p9a', 'p9b', t, '#1 | Press! | CRC', '#2 | Press! | CRC', '$# | %res@ | (RC', '^2 | #r*s! | C!&', current_b1_y, b2_y);
    t += pause;

    t = addCollisionAnim('p10a', 'p10b', t, '#1 | Press! | CRC', '#2 | Press! | CRC', '$# | %res@ | (RC', '^2 | #r*s! | C!&', current_b1_y, b2_y);
    t += pause;

    let t_lbt = t;
    addPacketAnim('p11a', t_lbt, 0.5, 2.0, '#1 | Press! | CRC', null, 1.0, start_x, current_b1_y, end_x, center_y);

    let b1_bot = { x: b1_x, y: current_b1_y + height / 2 };
    let b2_top = { x: b1_x, y: b2_y - height / 2 };
    addPacketAnim('p11b', t_lbt, 0.0, 0.5, '#1 | Press! | CRC', null, 1.0, b1_bot.x, b1_bot.y, b2_top.x, b2_top.y);

    let t_lbt_recv = t_lbt + 0.5 + 0.2;
    vid.add_transition(['listen_timer'], t_lbt_recv, 0.1, { opacity: 1 });
    vid.add_transition(['button2_box'], t_lbt_recv, 0.1, { text_opacity: 0.0 });
    vid.add_transition(['listen_timer'], t_lbt_recv, 3.0, { progress: 0.0 });

    let t_lbt_done = t_lbt_recv + 3.0; // t_lbt + 3.7
    vid.add_transition(['listen_timer'], t_lbt_done, 0.1, { opacity: 0 });
    vid.add_transition(['button2_box'], t_lbt_done, 0.1, { text_opacity: 1.0 });

    t = addPacketAnim('p12', t_lbt_done, 0.5, 2.0, '#2 | Press! | CRC', null, 1.0, start_x, b2_y, end_x, center_y);
    t += pause;

    vid.set_duration(t + 1);

    return vid;
}

function part6_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part6_video");

    vid.add_object('title', { opacity: 0, text: 'Encryption' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 350;

    let width = 180;
    let height = 90;
    let main_height = 180;
    let font_size = 22;

    let b1_x = center_x - spacing;
    let rx_x = center_x + spacing;

    vid.add_object('boxes', { opacity: 0, key_opacity: 0, rx_text_opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;

        let box1 = new DiagramBox({
            text: params.key_opacity > 0.5 ? 'Button\nBoard\n\n' : 'Button\nBoard',
            width, height: main_height, font_size,
            position: { x: b1_x, y: center_y },
        });
        box1.draw(ctx);

        if (params.key_opacity > 0) {
            ctx.save();
            ctx.globalAlpha = params.key_opacity;
            ctx.textDrawingMode = "glyph";
            ctx.font = '30px "Noto Color Emoji"';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('🔑', b1_x, center_y + 40);
            ctx.restore();
        }

        let rx_text = params.key_opacity > 0.5 ? 'Receiver\n\n' : 'Receiver';
        let box2 = new DiagramBox({
            text: rx_text,
            width, height: main_height, font_size,
            position: { x: rx_x, y: center_y },
        });
        box2.draw(ctx);

        if (params.key_opacity > 0) {
            ctx.save();
            ctx.globalAlpha = params.key_opacity;
            ctx.textDrawingMode = "glyph";
            ctx.font = '30px "Noto Color Emoji"';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('🔑', rx_x, center_y + 40);
            ctx.restore();
        }

        if (params.rx_text_opacity > 0) {
            ctx.save();
            ctx.globalAlpha = params.rx_text_opacity;
            ctx.font = '18px "Noto Sans"';
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.fillText('last_packet_counter = 2', rx_x, center_y + main_height / 2 + 30);
            ctx.restore();
        }
    });

    vid.add_object('hacker_box', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;
        let box = new DiagramBox({
            text: 'Hacker\n ',
            width, height, font_size,
            position: { x: center_x, y: center_y },
            background_color: '#ffcccc'
        });
        box.draw(ctx);

        ctx.save();
        ctx.globalAlpha = params.opacity;
        ctx.textDrawingMode = "glyph";
        ctx.font = '30px "Noto Color Emoji"';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('🥷', center_x, center_y + 15);
        ctx.restore();
    });

    vid.add_object('key_highlights', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;
        ctx.lineWidth = 4;
        ctx.strokeStyle = '#f00';
        ctx.setLineDash([5, 5]);

        let hw = 80;
        let hh = 80;
        ctx.strokeRect(b1_x - hw / 2, center_y + 40 - hh / 2, hw, hh);
        ctx.strokeRect(rx_x - hw / 2, center_y + 40 - hh / 2, hw, hh);
    });

    vid.add_object('pointer_arrow', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;
        ctx.fillStyle = '#f00';
        ctx.strokeStyle = '#f00';
        let startPt = { x: start_x + 130, y: center_y - 95 };
        let endPt = { x: start_x + 60, y: center_y - 31 };
        drawArrowPos(ctx, startPt, endPt, 3, 20, false);
    });

    vid.add_object('pointer_text', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;
        ctx.save();
        ctx.translate(start_x + 130, center_y - 135);
        draw_multiline_text(ctx, {
            text: "32-bit value\n(Max Value = 2^32)",
            font_size: 24,
            color: '#f00',
            text_align: 'center',
            text_baseline: 'middle'
        });
        ctx.restore();
    });

    function drawPacket(ctx, params, text, x, y) {
        ctx.save();
        ctx.translate(x, y);
        let lines = text.split('\n');
        let max_len = Math.max(...lines.map(l => l.length));
        let pwidth = 20 + (max_len * 9);
        let pheight = 20 + lines.length * 16;
        ctx.fillStyle = '#000';
        draw_box(ctx, pwidth, pheight);

        ctx.fillStyle = '#fff';
        ctx.font = `14px "Noto Sans Mono", monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        let start_y = -((lines.length - 1) * 16) / 2;
        lines.forEach((line, i) => {
            ctx.fillText(line, 0, start_y + (i * 16));
        });
        ctx.restore();
    }

    let p_id = 0;
    function addPacketAnim(t_start, dur, hold, delay, txt1, txt2, sx, sy, ex, ey, yOffsetEnd = 0, deflectStart = false) {
        let name = 'p' + (p_id++);
        vid.add_object(name, { opacity: 0, progress: 0.0, key_arrow_op: 0, key_arrow_len: 0, key_src_x: 0, txt_state: 0, deflect_p: 0 }, (ctx, params) => {
            ctx.globalAlpha = params.opacity;
            let px = sx + (ex - sx) * params.progress;
            let py = sy + (ey - sy) * params.progress;

            if (yOffsetEnd) {
                py += yOffsetEnd * params.progress;
            }

            if (deflectStart && params.deflect_p > 0) {
                px = ex - 100 + params.deflect_p * 300;
                py = ey - 100 - params.deflect_p * 400;
                ctx.save();
                ctx.translate(px, py);
                ctx.rotate(params.deflect_p * Math.PI * 6);
                ctx.translate(-px, -py);
            }

            let txt = (txt2 && params.txt_state > 0.5) ? txt2 : txt1;
            drawPacket(ctx, params, txt, px, py);

            if (params.key_arrow_len > 0 && params.key_arrow_op > 0) {
                ctx.globalAlpha = params.key_arrow_op;
                ctx.fillStyle = '#f00';
                ctx.strokeStyle = '#f00';
                let startPt = { x: params.key_src_x, y: center_y + 40 };
                let dest_x = params.key_src_x < px ? px - 40 : px + 40;
                let dest_y = py;
                let endPt = {
                    x: startPt.x + (dest_x - startPt.x) * params.key_arrow_len,
                    y: startPt.y + (dest_y - startPt.y) * params.key_arrow_len
                };
                if (params.key_arrow_len > 0.05) {
                    drawArrowPos(ctx, startPt, endPt, 3, 20, false);
                }
            }
            if (deflectStart && params.deflect_p > 0) ctx.restore();
        });

        vid.add_transition([name], t_start + delay, 0.2, { opacity: 1 });

        if (txt2 && hold > 0) {
            let arrow_t = t_start + delay + (hold * 0.2);
            vid.add_transition([name], arrow_t, 0.01, { key_arrow_op: 1, key_src_x: sx > center_x ? rx_x : b1_x });
            vid.add_transition([name], arrow_t, 0.2, { key_arrow_len: 1.0 });

            vid.add_transition([name], arrow_t + (hold * 0.4), 0.1, { txt_state: 1.0 });

            vid.add_transition([name], arrow_t + (hold * 0.6), 0.2, { key_arrow_op: 0 });
            vid.add_transition([name], arrow_t + (hold * 0.8), 0.01, { key_arrow_len: 0 });
        }

        let move_start = t_start + delay + hold;
        vid.add_transition([name], move_start, dur, { progress: 1.0 });

        if (deflectStart) {
            vid.add_transition([name], move_start + dur, 1.0, { deflect_p: 1.0, opacity: 0 });
            return move_start + dur + 1.0;
        }

        return move_start + dur;
    }

    let t = 0;
    let pause = 0.5;

    let start_x = b1_x + width / 2 + 20;
    let end_x = rx_x - width / 2 - 20;
    let h_x = center_x;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['boxes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['hacker_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    let t_end = addPacketAnim(t, 2.0, 0, 0, 'Press!', null, start_x, center_y, end_x, center_y);
    vid.add_transition(['p0'], t_end, 0.2, { opacity: 0 });
    t = t_end + 0.2 + pause;

    t_end = addPacketAnim(t, 1.0, 0, 0, 'Press!', null, h_x, center_y, end_x, center_y);
    vid.add_transition(['p1'], t_end, 0.2, { opacity: 0 });
    t = t_end + 0.2 + pause;

    vid.add_transition(['hacker_box'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    vid.add_transition(['boxes'], t, 0.5, { key_opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['key_highlights'], t, 0.2, { opacity: 1 });
    vid.add_transition(['key_highlights'], t + 1.0, 0.2, { opacity: 0 });
    t += 1.2 + pause;

    t_end = addPacketAnim(t, 2.0, 1.5, 0.5, 'Press!', '@F$*!', start_x, center_y, end_x, center_y);
    vid.add_transition(['p2'], t_end, 0.01, { key_arrow_op: 1, key_src_x: rx_x, key_arrow_len: 0.0 });
    vid.add_transition(['p2'], t_end, 0.2, { key_arrow_len: 1.0 });
    vid.add_transition(['p2'], t_end + 0.4, 0.1, { txt_state: 0.0 });
    vid.add_transition(['p2'], t_end + 0.8, 0.2, { key_arrow_op: 0 });
    vid.add_transition(['p2'], t_end + 1.5, 0.2, { opacity: 0 });
    t = t_end + 1.7 + pause;

    vid.add_transition(['hacker_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    t_end = addPacketAnim(t, 2.0, 0, 0.5, '@F$*!', null, start_x, center_y, end_x, center_y);
    t = t_end + pause;

    t_end = addPacketAnim(t, 1.0, 0, 0, '@F$*!', null, h_x, center_y, end_x, center_y, 50);
    t = t_end + pause;

    vid.add_transition(['p3', 'p4'], t, 0.2, { opacity: 0 });
    t += 0.2 + pause;

    t_end = addPacketAnim(t, 2.0, 1.5, 0.5, 'Counter: 1\nPress!', 'Counter: 1\n*E$#!', start_x, center_y - 30, end_x, center_y - 30);
    t = t_end + pause;

    t_end = addPacketAnim(t, 2.0, 1.5, 0.5, 'Counter: 2\nPress!', 'Counter: 2\n&^@$!', start_x, center_y + 30, end_x, center_y + 30);
    t = t_end + pause;

    vid.add_transition(['boxes'], t, 0.5, { rx_text_opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['p5', 'p6'], t, 0.2, { opacity: 0 });
    t += 0.2 + pause;

    t_end = addPacketAnim(t, 1.5, 0, 0.5, 'Counter: 1\n*E$#!', null, h_x, center_y, end_x, center_y, 0, true);
    t = t_end + pause;

    let p_c3 = 'p' + (p_id++);
    vid.add_object(p_c3, { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;
        drawPacket(ctx, params, 'Counter: 3\nPress!', start_x, center_y);
    });
    vid.add_transition([p_c3], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['pointer_arrow', 'pointer_text'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.set_duration(t + 1);
    return vid;
}

function part6_key_exchange_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part6_key_exchange_video");

    vid.add_object('title', { opacity: 0, text: 'Key Exchange' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 350;

    let width = 180;
    let main_height = 180;
    let font_size = 22;

    let b1_x = center_x - spacing;
    let rx_x = center_x + spacing;

    vid.add_object('boxes', { opacity: 0, key1_size: 0, key2_size: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;

        let box1 = new DiagramBox({
            text: 'Button\nBoard\n\n',
            width, height: main_height, font_size,
            position: { x: b1_x, y: center_y },
        });
        box1.draw(ctx);

        if (params.key1_size > 0.1) {
            ctx.save();
            ctx.textDrawingMode = "glyph";
            ctx.font = `${params.key1_size}px "Noto Color Emoji"`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('🔑', b1_x, center_y + 40);
            ctx.restore();
        }

        let box2 = new DiagramBox({
            text: 'Receiver\n\n',
            width, height: main_height, font_size,
            position: { x: rx_x, y: center_y },
        });
        box2.draw(ctx);

        if (params.key2_size > 0.1) {
            ctx.save();
            ctx.textDrawingMode = "glyph";
            ctx.font = `${params.key2_size}px "Noto Color Emoji"`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('🔑', rx_x, center_y + 40);
            ctx.restore();
        }
    });

    function drawKeyPacket(ctx, params, x, y) {
        ctx.save();
        ctx.translate(x, y);
        let pwidth = 50;
        let pheight = 40;
        ctx.fillStyle = '#000';
        draw_box(ctx, pwidth, pheight);

        ctx.fillStyle = '#fff';
        ctx.textDrawingMode = "glyph";
        ctx.font = `20px "Noto Color Emoji"`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('🔑', 0, 0);
        ctx.restore();
    }

    let p_id = 0;
    function addKeyPacket(t_start, sx, sy, ex, ey) {
        let name = 'kp' + (p_id++);
        vid.add_object(name, { opacity: 0, progress: 0.0 }, (ctx, params) => {
            ctx.globalAlpha = params.opacity;
            let px = sx + (ex - sx) * params.progress;
            let py = sy + (ey - sy) * params.progress;
            drawKeyPacket(ctx, params, px, py);
        });

        vid.add_transition([name], t_start, 0.2, { opacity: 1 });
        let dur = 1.5;
        vid.add_transition([name], t_start, dur, { progress: 1.0 });
        vid.add_transition([name], t_start + dur, 0.2, { opacity: 0 });
        return t_start + dur + 0.2;
    }

    let t = 0;
    let pause = 0.5;

    let start_x = b1_x + width / 2;
    let end_x = rx_x - width / 2;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['boxes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    let t_end = addKeyPacket(t, start_x, center_y, end_x, center_y);
    t = t_end - 0.2;
    vid.add_transition(['boxes'], t, 0.5, { key2_size: 15 });
    t += 0.5 + pause;

    t_end = addKeyPacket(t, end_x, center_y, start_x, center_y);
    t = t_end - 0.2;
    vid.add_transition(['boxes'], t, 0.5, { key1_size: 15 });
    t += 0.5 + pause;

    t_end = addKeyPacket(t, start_x, center_y, end_x, center_y);
    t = t_end - 0.2;
    vid.add_transition(['boxes'], t, 0.5, { key2_size: 30 });
    t += 0.5 + pause;

    t_end = addKeyPacket(t, end_x, center_y, start_x, center_y);
    t = t_end - 0.2;
    vid.add_transition(['boxes'], t, 0.5, { key1_size: 30 });
    t += 0.5 + pause;

    vid.set_duration(t + 1);
    return vid;
}

function part6_ack_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part6_ack_video");

    vid.add_object('title', { opacity: 0, text: 'Optimistic Acknowledgement' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 350;

    let width = 180;
    let height = 90;
    let font_size = 22;

    let top_y = center_y - 80;
    let bot_y = center_y + 150;

    let b1_x = center_x - spacing;
    let rx_x = center_x + spacing;

    vid.add_object('boxes', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;

        let box1 = new DiagramBox({
            text: 'Button\nBoard',
            width, height, font_size,
            position: { x: b1_x, y: top_y },
        });
        box1.draw(ctx);

        let box2 = new DiagramBox({
            text: 'Receiver',
            width, height, font_size,
            position: { x: rx_x, y: top_y },
        });
        box2.draw(ctx);

        let box3 = new DiagramBox({
            text: 'Computer',
            width, height, font_size,
            position: { x: rx_x, y: bot_y },
        });
        box3.draw(ctx);

        ctx.lineWidth = 4;
        ctx.strokeStyle = '#000';
        ctx.beginPath();
        ctx.moveTo(rx_x, top_y + height / 2);
        ctx.lineTo(rx_x, bot_y - height / 2);
        ctx.stroke();
    });

    function drawPacket(ctx, params, text, x, y) {
        ctx.save();
        ctx.translate(x, y);
        let lines = text.split('\n');
        let max_len = Math.max(...lines.map(l => l.length));
        let pwidth = 20 + (max_len * 9);
        let pheight = 20 + lines.length * 16;
        ctx.fillStyle = '#000';
        draw_box(ctx, pwidth, pheight);

        ctx.fillStyle = '#fff';
        ctx.font = `14px "Noto Sans Mono", monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        let start_y = -((lines.length - 1) * 16) / 2;
        lines.forEach((line, i) => {
            ctx.fillText(line, 0, start_y + (i * 16));
        });
        ctx.restore();
    }

    let p_id = 0;
    function addPacketAnim(t_start, dur, txt, sx, sy, ex, ey) {
        let name = 'p' + (p_id++);
        vid.add_object(name, { opacity: 0, progress: 0.0 }, (ctx, params) => {
            ctx.globalAlpha = params.opacity;
            let px = sx + (ex - sx) * params.progress;
            let py = sy + (ey - sy) * params.progress;
            drawPacket(ctx, params, txt, px, py);
        });

        vid.add_transition([name], t_start, 0.2, { opacity: 1 });
        vid.add_transition([name], t_start + 0.2, dur, { progress: 1.0 });

        let t_end = t_start + 0.2 + dur;
        vid.add_transition([name], t_end + 0.5, 0.2, { opacity: 0 });

        return t_end + 0.5;
    }

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['boxes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    let pr_b_rx = b1_x + width / 2 + 20;
    let pr_r_lx = rx_x - width / 2 - 20;
    let pr_r_bx = rx_x;
    let pr_r_by = top_y + height / 2 + 20;
    let pr_c_ty = bot_y - height / 2 - 20;

    let t_end = addPacketAnim(t, 2.0, "Press!", pr_b_rx, top_y, pr_r_lx, top_y);
    t = t_end + pause;

    t_end = addPacketAnim(t, 2.0, "Ack", pr_r_lx, top_y, pr_b_rx, top_y);
    t = t_end + pause;

    t_end = addPacketAnim(t, 2.0, "Press!", pr_r_bx, pr_r_by, pr_r_bx, pr_c_ty);
    t = t_end + pause;

    vid.set_duration(t + 1);
    return vid;
}

function part7_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part7_video");

    vid.add_object('title', { opacity: 0, text: 'Battery Level Reporting' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 350;

    let width = 180;
    let main_height = 180;
    let font_size = 22;

    let b1_x = center_x - spacing;
    let rx_x = center_x + spacing;

    vid.add_object('boxes', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;

        let box1 = new DiagramBox({
            text: 'Button\nBoard\n\n',
            width, height: main_height, font_size,
            position: { x: b1_x, y: center_y },
        });
        box1.draw(ctx);

        let box2 = new DiagramBox({
            text: 'Receiver\n\n',
            width, height: main_height, font_size,
            position: { x: rx_x, y: center_y },
        });
        box2.draw(ctx);
    });

    vid.add_object('progress_timer', { opacity: 0, progress: 1.0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        ctx.translate(b1_x, center_y + 40);

        let radius = 20;
        ctx.lineWidth = 6;
        ctx.strokeStyle = '#003399';
        ctx.beginPath();
        let angle = params.progress * Math.PI * 2;
        ctx.arc(0, 0, radius, -Math.PI / 2, -Math.PI / 2 + angle, false);
        ctx.stroke();

        ctx.restore();
    });

    function drawPacket(ctx, params, text, x, y) {
        ctx.save();
        ctx.translate(x, y);
        let lines = text.split('\n');
        let max_len = Math.max(...lines.map(l => l.length));
        let pwidth = 20 + (max_len * 9);
        let pheight = 20 + lines.length * 16;
        ctx.fillStyle = '#000';
        draw_box(ctx, pwidth, pheight);

        ctx.fillStyle = '#fff';
        ctx.textDrawingMode = "glyph";
        ctx.font = `14px "Noto Sans Mono", monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        let start_y = -((lines.length - 1) * 16) / 2;
        lines.forEach((line, i) => {
            ctx.fillText(line, 0, start_y + (i * 16));
        });
        ctx.restore();
    }

    let p_id = 0;
    function addPacketAnim(t_start, dur, txt, sx, sy, ex, ey) {
        let name = 'p' + (p_id++);
        vid.add_object(name, { opacity: 0, progress: 0.0 }, (ctx, params) => {
            ctx.globalAlpha = params.opacity;
            let px = sx + (ex - sx) * params.progress;
            let py = sy + (ey - sy) * params.progress;
            drawPacket(ctx, params, txt, px, py);
        });

        vid.add_transition([name], t_start, 0.2, { opacity: 1 });
        vid.add_transition([name], t_start + 0.2, dur, { progress: 1.0 });

        let t_end = t_start + 0.2 + dur;
        vid.add_transition([name], t_end + 0.3, 0.2, { opacity: 0 });

        return t_end + 0.3;
    }

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title', 'boxes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    let start_x = b1_x + width / 2 + 20;
    let end_x = rx_x - width / 2 - 20;

    let battery = 90;
    for (let i = 0; i < 5; i++) {
        let t_end = addPacketAnim(t, 2.0, `Battery:\n${battery}%`, start_x, center_y, end_x, center_y);
        t = t_end;
        battery -= 10;

        if (i < 4) {
            vid.add_transition(['progress_timer'], t, 0.01, { progress: 1.0 });
            vid.add_transition(['progress_timer'], t, 0.2, { opacity: 1 });
            t += 0.2;

            vid.add_transition(['progress_timer'], t, 1.0, { progress: 0.0 });
            t += 1.0;

            vid.add_transition(['progress_timer'], t, 0.2, { opacity: 0 });
            t += 0.2;
        }
    }

    vid.set_duration(t + 1);
    return vid;
}

function part11_weather_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part11_weather_video");

    vid.add_object('title', { opacity: 0, text: 'Weather Monitoring' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 350;

    let width = 180;
    let main_height = 180;
    let font_size = 22;

    let b1_x = center_x - spacing;
    let rx_x = center_x + spacing;

    vid.add_object('boxes', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;

        let box1 = new DiagramBox({
            text: 'Button\nBoard\n\n',
            width, height: main_height, font_size,
            position: { x: b1_x, y: center_y },
        });
        box1.draw(ctx);

        let box2 = new DiagramBox({
            text: 'Receiver\n\n',
            width, height: main_height, font_size,
            position: { x: rx_x, y: center_y },
        });
        box2.draw(ctx);
    });

    vid.add_object('progress_timer', { opacity: 0, progress: 1.0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        ctx.translate(b1_x, center_y + 40);

        let radius = 20;
        ctx.lineWidth = 6;
        ctx.strokeStyle = '#003399';
        ctx.beginPath();
        let angle = params.progress * Math.PI * 2;
        ctx.arc(0, 0, radius, -Math.PI / 2, -Math.PI / 2 + angle, false);
        ctx.stroke();

        ctx.restore();
    });

    function drawPacket(ctx, params, text, x, y) {
        ctx.save();
        ctx.translate(x, y);
        let lines = text.split('\n');
        let max_len = Math.max(...lines.map(l => l.length));
        let pwidth = 20 + (max_len * 9);
        let pheight = 20 + lines.length * 16;
        ctx.fillStyle = '#000';
        draw_box(ctx, pwidth, pheight);

        ctx.textDrawingMode = "glyph";
        ctx.fillStyle = '#fff';
        ctx.font = `14px "Noto Sans Mono", monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        let start_y = -((lines.length - 1) * 16) / 2;
        lines.forEach((line, i) => {
            ctx.fillText(line, 0, start_y + (i * 16));
        });
        ctx.restore();
    }

    let p_id = 0;
    function addPacketAnim(t_start, dur, txt, sx, sy, ex, ey) {
        let name = 'p' + (p_id++);
        vid.add_object(name, { opacity: 0, progress: 0.0 }, (ctx, params) => {
            ctx.globalAlpha = params.opacity;
            let px = sx + (ex - sx) * params.progress;
            let py = sy + (ey - sy) * params.progress;
            drawPacket(ctx, params, txt, px, py);
        });

        vid.add_transition([name], t_start, 0.2, { opacity: 1 });
        vid.add_transition([name], t_start + 0.2, dur, { progress: 1.0 });

        let t_end = t_start + 0.2 + dur;
        vid.add_transition([name], t_end + 0.3, 0.2, { opacity: 0 });

        return t_end + 0.3;
    }

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title', 'boxes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    let start_x = b1_x + width / 2 + 20;
    let end_x = rx_x - width / 2 - 20;

    let battery = 90;
    for (let i = 0; i < 5; i++) {
        let txt = `Battery: ${battery}%\nTemperature: 25C\nHumidity: 50%`;
        let t_end = addPacketAnim(t, 2.0, txt, start_x, center_y, end_x, center_y);
        t = t_end;
        battery -= 10;

        if (i < 4) {
            vid.add_transition(['progress_timer'], t, 0.01, { progress: 1.0 });
            vid.add_transition(['progress_timer'], t, 0.2, { opacity: 1 });
            t += 0.2;

            vid.add_transition(['progress_timer'], t, 1.0, { progress: 0.0 });
            t += 1.0;

            vid.add_transition(['progress_timer'], t, 0.2, { opacity: 0 });
            t += 0.2;
        }
    }

    vid.set_duration(t + 1);
    return vid;
}

function part11_eink_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part11_eink_video");

    vid.add_object('title', { opacity: 0, text: 'EInk Display Updating' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y;
    let spacing = 350;

    let width = 180;
    let main_height = 180;
    let font_size = 22;

    let b1_x = center_x - spacing;
    let rx_x = center_x + spacing;

    vid.add_object('boxes', { opacity: 0 }, (ctx, params) => {
        ctx.globalAlpha = params.opacity;

        let box1 = new DiagramBox({
            text: 'Button\nBoard\n\n',
            width, height: main_height, font_size,
            position: { x: b1_x, y: center_y },
        });
        box1.draw(ctx);

        let box2 = new DiagramBox({
            text: 'Receiver\n\n',
            width, height: main_height, font_size,
            position: { x: rx_x, y: center_y },
        });
        box2.draw(ctx);
    });

    vid.add_object('progress_timer', { opacity: 0, progress: 1.0 }, (ctx, params) => {
        ctx.save();
        ctx.globalAlpha = params.opacity;
        ctx.translate(b1_x, center_y + 40);

        let radius = 20;
        ctx.lineWidth = 6;
        ctx.strokeStyle = '#003399';
        ctx.beginPath();
        let angle = params.progress * Math.PI * 2;
        ctx.arc(0, 0, radius, -Math.PI / 2, -Math.PI / 2 + angle, false);
        ctx.stroke();

        ctx.restore();
    });

    function drawPacket(ctx, params, text, x, y) {
        ctx.save();
        ctx.translate(x, y);
        let lines = text.split('\n');
        let max_len = Math.max(...lines.map(l => l.length));
        let pwidth = 20 + (max_len * 9);
        let pheight = 20 + lines.length * 16;
        ctx.fillStyle = '#000';
        draw_box(ctx, pwidth, pheight);

        ctx.textDrawingMode = "glyph";
        ctx.fillStyle = '#fff';
        ctx.font = `14px "Noto Sans Mono", monospace`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        let start_y = -((lines.length - 1) * 16) / 2;
        lines.forEach((line, i) => {
            ctx.fillText(line, 0, start_y + (i * 16));
        });
        ctx.restore();
    }

    let p_id = 0;
    function addPacketAnim(t_start, dur, txt, sx, sy, ex, ey) {
        let name = 'p' + (p_id++);
        vid.add_object(name, { opacity: 0, progress: 0.0 }, (ctx, params) => {
            ctx.globalAlpha = params.opacity;
            let px = sx + (ex - sx) * params.progress;
            let py = sy + (ey - sy) * params.progress;
            drawPacket(ctx, params, txt, px, py);
        });

        vid.add_transition([name], t_start, 0.2, { opacity: 1 });
        vid.add_transition([name], t_start + 0.2, dur, { progress: 1.0 });

        let t_end = t_start + 0.2 + dur;
        vid.add_transition([name], t_end + 0.3, 0.2, { opacity: 0 });

        return t_end + 0.3;
    }

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title', 'boxes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    let start_x = b1_x + width / 2 + 20;
    let end_x = rx_x - width / 2 - 20;

    let battery = 90;
    for (let i = 0; i < 5; i++) {
        let txt = `Battery:\n${battery}%`;
        let t_end = addPacketAnim(t, 2.0, txt, start_x, center_y, end_x, center_y);
        t = t_end;
        battery -= 10;

        let ack_txt = "Ack\nData: Display Text";
        t_end = addPacketAnim(t, 2.0, ack_txt, end_x, center_y, start_x, center_y);
        t = t_end;

        if (i < 4) {
            vid.add_transition(['progress_timer'], t, 0.01, { progress: 1.0 });
            vid.add_transition(['progress_timer'], t, 0.2, { opacity: 1 });
            t += 0.2;

            vid.add_transition(['progress_timer'], t, 1.0, { progress: 0.0 });
            t += 1.0;

            vid.add_transition(['progress_timer'], t, 0.2, { opacity: 0 });
            t += 0.2;
        }
    }

    vid.set_duration(t + 1);
    return vid;
}

function part2_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part2_plan");

    vid.add_object('title', { opacity: 0, text: 'The Plan' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 4);

    let width = 180;
    let height = 90;
    let font_size = 22;

    let mcu_box = new DiagramBox({
        text: 'Microcontroller',
        width,
        height,
        font_size,
        position: box_grid.cell(1, 1).center(),
    });
    vid.add_object('mcu', { opacity: 0 }, (ctx, params) => {
        mcu_box.draw(ctx);
    });

    let button_box = new DiagramBox({
        text: 'Button',
        width,
        height,
        font_size,
        position: box_grid.cell(1, 0).center(),
    });
    vid.add_object('button', { opacity: 0 }, (ctx, params) => {
        button_box.draw(ctx);

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            button_box.right_center(),
            mcu_box.left_center(),
            2, 20, false
        );
    });

    let sensor_box = new DiagramBox({
        text: 'Sensors',
        width,
        height,
        font_size,
        position: box_grid.cell(0, 0).center(),
    });
    vid.add_object('sensors', { opacity: 0 }, (ctx, params) => {
        sensor_box.draw(ctx);

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            { x: sensor_box.right_center().x, y: sensor_box.bottom_center().y },
            { x: mcu_box.left_center().x, y: mcu_box.top_center().y },
            2, 20, false
        );
    });

    let radio_box = new DiagramBox({
        text: 'Radio',
        width,
        height,
        font_size,
        position: box_grid.cell(2, 1).center(),
    });
    vid.add_object('radio', { opacity: 0 }, (ctx, params) => {
        radio_box.draw(ctx);

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            mcu_box.bottom_center(),
            radio_box.top_center(),
            2, 20, false
        );
    });

    let receiver_box = new DiagramBox({
        text: 'Receiver',
        width,
        height,
        font_size,
        position: box_grid.cell(2, 2).center(),
    });
    vid.add_object('receiver', { opacity: 0 }, (ctx, params) => {
        receiver_box.draw(ctx);

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            radio_box.right_center(),
            receiver_box.left_center(),
            2, 20, false
        );
    });

    let computer_box = new DiagramBox({
        text: 'Computer',
        width,
        height,
        font_size,
        position: box_grid.cell(2, 3).center(),
    });
    vid.add_object('computer', { opacity: 0 }, (ctx, params) => {
        computer_box.draw(ctx);

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            receiver_box.right_center(),
            computer_box.left_center(),
            2, 20, false
        );
    });

    let api_box = new DiagramBox({
        text: 'Lights API',
        width,
        height,
        font_size,
        position: box_grid.cell(1, 3).center(),
    });
    vid.add_object('api', { opacity: 0 }, (ctx, params) => {
        api_box.draw(ctx);

        {
            ctx.save();

            let pos = box_grid.cell(0, 3).center();

            ctx.translate(pos.x, pos.y);

            ctx.textDrawingMode = "glyph";
            ctx.font = '50px "Noto Color Emoji"';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';

            ctx.fillText('💡', 0, 0);

            ctx.restore();
        }

        ctx.fillStyle = '#000';
        drawArrowPos(
            ctx,
            computer_box.top_center(),
            api_box.bottom_center(),
            2, 20, false
        );

        let pos = box_grid.cell(0, 3).center();
        pos.y += 40;

        drawArrowPos(
            ctx,
            api_box.top_center(),
            pos,
            2, 20, false
        );

    });

    let battery_box = new DiagramBox({
        text: 'Battery',
        width,
        height,
        font_size,
        position: box_grid.cell(0, 1).center(),
    });
    vid.add_object('battery', { opacity: 0 }, (ctx, params) => {
        battery_box.draw(ctx);
    });

    vid.add_object('outline', { opacity: 0 }, (ctx, params) => {
        let x_min = sensor_box.left_center().x;
        let y_min = sensor_box.top_center().y;
        let x_max = radio_box.right_center().x;
        let y_max = radio_box.bottom_center().y;

        let pad = 10;

        let height = (y_max - y_min) + 2 * pad;

        let box = new DiagramBox({
            text: 'Low Power Stuff',
            font_size: 22,
            text_color: '#f00',
            background_color: 'rgba(0,0,0,0)',
            width: (x_max - x_min) + 2 * pad,
            height,
            position: { x: (x_max + x_min) / 2, y: (y_min + y_max) / 2 },
            stroke_color: '#f00',
            text_offset: { x: 40, y: -(height / 2) - 20 }
        });

        ctx.lineWidth = 4;
        ctx.setLineDash([5, 5]);
        box.draw(ctx);
    });

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['mcu'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['button'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['sensors'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['radio'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['receiver'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['computer'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['api'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['battery'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['outline'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.set_duration(t);

    return vid;
}

function part5_checkmarks(canvas) {
    let vid = new Timeline();

    vid.set_name("part5_checkmarks");

    vid.add_object('title', { opacity: 1, text: 'The Plan' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 4);

    let width = 180;
    let height = 90;
    let font_size = 22;

    let mcu_box = new DiagramBox({
        text: 'Microcontroller',
        width, height, font_size,
        position: box_grid.cell(1, 1).center(),
    });
    vid.add_object('mcu', { opacity: 1 }, (ctx, params) => {
        mcu_box.draw(ctx);
    });

    let button_box = new DiagramBox({
        text: 'Button',
        width, height, font_size,
        position: box_grid.cell(1, 0).center(),
    });
    vid.add_object('button', { opacity: 1 }, (ctx, params) => {
        button_box.draw(ctx);
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, button_box.right_center(), mcu_box.left_center(), 2, 20, false);
    });

    let sensor_box = new DiagramBox({
        text: 'Sensors',
        width, height, font_size,
        position: box_grid.cell(0, 0).center(),
    });
    vid.add_object('sensors', { opacity: 1 }, (ctx, params) => {
        sensor_box.draw(ctx);
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, { x: sensor_box.right_center().x, y: sensor_box.bottom_center().y }, { x: mcu_box.left_center().x, y: mcu_box.top_center().y }, 2, 20, false);
    });

    let radio_box = new DiagramBox({
        text: 'Radio',
        width, height, font_size,
        position: box_grid.cell(2, 1).center(),
    });
    vid.add_object('radio', { opacity: 1 }, (ctx, params) => {
        radio_box.draw(ctx);
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, mcu_box.bottom_center(), radio_box.top_center(), 2, 20, false);
    });

    let receiver_box = new DiagramBox({
        text: 'Receiver',
        width, height, font_size,
        position: box_grid.cell(2, 2).center(),
    });
    vid.add_object('receiver', { opacity: 1 }, (ctx, params) => {
        receiver_box.draw(ctx);
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, radio_box.right_center(), receiver_box.left_center(), 2, 20, false);
    });

    let computer_box = new DiagramBox({
        text: 'Computer',
        width, height, font_size,
        position: box_grid.cell(2, 3).center(),
    });
    vid.add_object('computer', { opacity: 1 }, (ctx, params) => {
        computer_box.draw(ctx);
        ctx.fillStyle = '#000';
        drawArrowPos(ctx, receiver_box.right_center(), computer_box.left_center(), 2, 20, false);
    });

    let api_box = new DiagramBox({
        text: 'Lights API',
        width, height, font_size,
        position: box_grid.cell(1, 3).center(),
    });
    vid.add_object('api', { opacity: 1 }, (ctx, params) => {
        api_box.draw(ctx);
        ctx.save();
        let pos = box_grid.cell(0, 3).center();
        ctx.translate(pos.x, pos.y);
        ctx.textDrawingMode = "glyph";
        ctx.font = '50px "Noto Color Emoji"';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('💡', 0, 0);
        ctx.restore();

        ctx.fillStyle = '#000';
        drawArrowPos(ctx, computer_box.top_center(), api_box.bottom_center(), 2, 20, false);

        let pos2 = box_grid.cell(0, 3).center();
        pos2.y += 40;
        drawArrowPos(ctx, api_box.top_center(), pos2, 2, 20, false);
    });

    let battery_box = new DiagramBox({
        text: 'Battery',
        width, height, font_size,
        position: box_grid.cell(0, 1).center(),
    });
    vid.add_object('battery', { opacity: 1 }, (ctx, params) => {
        battery_box.draw(ctx);
    });

    vid.add_object('check_mcu', { opacity: 0, scale: 2.0 }, (ctx, params) => {
        ctx.save();
        let pos = mcu_box.position();
        ctx.translate(pos.x, pos.y);
        ctx.scale(params.scale, params.scale);

        ctx.font = 'bold 60px "Noto Sans", sans-serif';
        ctx.fillStyle = '#0a0';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('✔', 0, 0);

        ctx.lineWidth = 2;
        ctx.strokeStyle = '#fff';
        ctx.strokeText('✔', 0, 0);

        ctx.restore();
    });

    vid.add_object('check_button', { opacity: 0, scale: 2.0 }, (ctx, params) => {
        ctx.save();
        let pos = button_box.position();
        ctx.translate(pos.x, pos.y);
        ctx.scale(params.scale, params.scale);

        ctx.font = 'bold 60px "Noto Sans", sans-serif';
        ctx.fillStyle = '#0a0';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('✔', 0, 0);

        ctx.lineWidth = 2;
        ctx.strokeStyle = '#fff';
        ctx.strokeText('✔', 0, 0);

        ctx.restore();
    });

    vid.add_object('new_outline', { opacity: 0 }, (ctx, params) => {
        let x_min = radio_box.left_center().x;
        let y_min = Math.min(radio_box.top_center().y, receiver_box.top_center().y);
        let x_max = receiver_box.right_center().x;
        let y_max = Math.max(radio_box.bottom_center().y, receiver_box.bottom_center().y);

        let pad = 10;
        let height = (y_max - y_min) + 2 * pad;
        let pwidth = (x_max - x_min) + 2 * pad;

        let box = new DiagramBox({
            text: '',
            background_color: 'rgba(0,0,0,0)',
            width: pwidth,
            height: height,
            position: { x: (x_max + x_min) / 2, y: (y_min + y_max) / 2 },
            stroke_color: '#f00'
        });

        ctx.lineWidth = 4;
        ctx.setLineDash([5, 5]);
        box.draw(ctx);
    });

    let pause = 0.5;
    let t = 0.5;

    vid.add_transition(['check_mcu'], t, 0.3, { opacity: 1, scale: 1.0 });
    t += 0.3 + pause;

    vid.add_transition(['check_button'], t, 0.3, { opacity: 1, scale: 1.0 });
    t += 0.3 + pause;

    vid.add_transition(['new_outline'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.set_duration(t + 1);

    return vid;
}