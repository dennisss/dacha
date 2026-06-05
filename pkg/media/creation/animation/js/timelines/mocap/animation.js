import { Timeline, draw_title, deg2rad, draw_box, slide_body_grid, DiagramBox, WireBundle, Wire, shallow_copy, draw_multiline_text, draw_box_text } from '../../utils.js';
import { hexToRgba } from '../../hex_to_rgba.js';
import { drawArrow, drawArrowPos } from '../../arrow.js';
import { getPointAtY } from '../../y_point.js';
import { drawPolyline, drawSequentialChains, drawShearedSquare } from '../../sheared_square.js';
// import { math_to_img, math_scale } from '../../mathjax.js';
import { drawCenteredTable } from '../../centered_table.js';
import { draw_graph } from '../3d_printer/motion_animation.js';
import { getInterpolatedY } from '../../linear_interp.js';
import { getObjectAlpha } from '../../staggered_fade.js';

function lerp(start, end, progress) {
    return start + (end - start) * progress;
}

export async function configure(canvas) {
    // return part2_triangulation_video(canvas);
    // return part2_architecture_video(canvas);
    // return part3_video(canvas);
    // return part4_video(canvas);
    // return part5_video(canvas);
    let vid = part10_life(canvas);
    return vid;
}

function part2_architecture_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part2_architecture");

    vid.add_object('title', { opacity: 0, text: 'System Architecture' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let cx = body_grid.left_center().x + (body_grid.width() * 2 / 3) / 2;
    let cy = body_grid.center().y;

    let cams = [
        { x: cx - 180, y: cy - 180, angle: Math.PI / 2, side: 'top' },
        { x: cx, y: cy - 180, angle: Math.PI / 2, side: 'top' },
        { x: cx + 180, y: cy - 180, angle: Math.PI / 2, side: 'top' },
        { x: cx - 180, y: cy + 180, angle: -Math.PI / 2, side: 'bottom' },
        { x: cx, y: cy + 180, angle: -Math.PI / 2, side: 'bottom' },
        { x: cx + 180, y: cy + 180, angle: -Math.PI / 2, side: 'bottom' },
        { x: cx - 280, y: cy - 100, angle: 0, side: 'left' },
        { x: cx - 280, y: cy, angle: 0, side: 'left' },
        { x: cx - 280, y: cy + 100, angle: 0, side: 'left' },
        { x: cx + 280, y: cy - 100, angle: Math.PI, side: 'right' },
        { x: cx + 280, y: cy, angle: Math.PI, side: 'right' },
        { x: cx + 280, y: cy + 100, angle: Math.PI, side: 'right' }
    ];

    let comp_x = body_grid.right_center().x - 75;
    let comp_y = body_grid.center().y;
    let comp_box = new DiagramBox({
        text: 'Computer',
        width: 150,
        height: 300,
        font_size: 20,
        position: { x: comp_x, y: comp_y }
    });

    let comp_top = comp_y - 150;
    let comp_bottom = comp_y + 150;
    let comp_left = comp_x - 75;

    let top_idx = 0;
    let bottom_idx = 0;
    let left_idx = 0;
    let right_idx = 0;

    let wires = cams.map((cam, i) => {
        let path = [];
        if (cam.side === 'top') {
            let start = { x: cam.x, y: cam.y - 20 };
            let y_up = cy - 210 - (top_idx * 10);
            let x_right = comp_x - 50 + (top_idx * 20);
            path.push(start);
            path.push({ x: start.x, y: y_up });
            path.push({ x: x_right, y: y_up });
            path.push({ x: x_right, y: comp_top });
            top_idx++;
        } else if (cam.side === 'bottom') {
            let start = { x: cam.x, y: cam.y + 20 };
            let y_down = cy + 210 + (bottom_idx * 10);
            let x_right = comp_x - 50 + (bottom_idx * 20);
            path.push(start);
            path.push({ x: start.x, y: y_down });
            path.push({ x: x_right, y: y_down });
            path.push({ x: x_right, y: comp_bottom });
            bottom_idx++;
        } else if (cam.side === 'left') {
            let start = { x: cam.x - 20, y: cam.y };
            path.push(start);
            path.push({ x: -20, y: start.y });
            left_idx++;
        } else if (cam.side === 'right') {
            let start = { x: cam.x + 20, y: cam.y };
            path.push(start);
            path.push({ x: comp_left, y: start.y });
            right_idx++;
        }
        return path;
    });

    vid.add_object('wires', { opacity: 1, non_right_opacity: 1, progress: 0 }, (ctx, params) => {
        if (params.progress > 0) {
            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            wires.forEach((wire, i) => {
                let is_right_cam = cams[i].side === 'right';
                ctx.save();
                ctx.globalAlpha *= params.opacity;
                if (!is_right_cam) ctx.globalAlpha *= params.non_right_opacity;
                drawSequentialChains(ctx, [wire], params.progress);
                ctx.restore();
            });
            ctx.restore();
        }
    });

    vid.add_object('cameras', { opacity: 0, non_right_opacity: 1, cam_slide_progress: 0, cam_scale_progress: 0, cam_scale_progress_2: 0, cam_spread_progress: 0 }, (ctx, params) => {
        let target_x = body_grid.left_center().x + 110 + lerp(0, 20, params.cam_scale_progress_2);
        let box_w = lerp(40, 140, params.cam_scale_progress) + lerp(0, 40, params.cam_scale_progress_2);
        let box_h = lerp(40, 80, params.cam_scale_progress) + lerp(0, 40, params.cam_scale_progress_2);
        let f_base = lerp(10, 20, params.cam_scale_progress);
        let f_tip = lerp(20, 40, params.cam_scale_progress);
        let f_len = lerp(20, 40, params.cam_scale_progress);
        let f_x = box_w / 2;

        cams.forEach(cam => {
            if (cam.side !== 'right' && params.non_right_opacity <= 0) return;

            ctx.save();
            let cur_x = (cam.side === 'right') ? lerp(cam.x, target_x, params.cam_slide_progress) : cam.x;
            let cam_y = cam.y;
            if (cam.side === 'right') {
                if (cam.y < cy) cam_y = lerp(cam.y, cy - 150, params.cam_spread_progress);
                else if (cam.y > cy) cam_y = lerp(cam.y, cy + 150, params.cam_spread_progress);
            }
            ctx.translate(cur_x, cam_y);
            ctx.rotate(cam.angle);

            if (cam.side !== 'right') {
                ctx.globalAlpha *= params.non_right_opacity;
            }

            ctx.fillStyle = '#aaccee';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            draw_box(ctx, box_w, box_h);

            ctx.fillStyle = '#ddf';
            ctx.beginPath();
            ctx.moveTo(f_x, f_base);
            ctx.lineTo(f_x + f_len, f_tip);
            ctx.lineTo(f_x + f_len, -f_tip);
            ctx.lineTo(f_x, -f_base);
            ctx.closePath();
            ctx.fill();
            ctx.stroke();

            ctx.restore();
        });
    });

    let markers = [
        { x: cx - 80, y: cy + 20 },
        { x: cx + 120, y: cy + 90 },
        { x: cx + 60, y: cy - 40 }
    ];

    let ray_origins = cams.map(cam => {
        return {
            x: cam.x + Math.cos(cam.angle) * 40,
            y: cam.y + Math.sin(cam.angle) * 40
        };
    });

    let rays_to_draw = [];
    markers.forEach(marker => {
        let sorted_cams = cams.map((cam, i) => {
            let origin = ray_origins[i];
            let dist = Math.hypot(marker.x - origin.x, marker.y - origin.y);
            return { cam, origin, dist };
        }).sort((a, b) => a.dist - b.dist);

        for (let i = 0; i < 3; i++) {
            rays_to_draw.push({
                start: sorted_cams[i].origin,
                end: marker,
                side: sorted_cams[i].cam.side
            });
        }
    });

    vid.add_object('markers_and_rays', { opacity: 0, non_right_opacity: 1, rays_progress: 0 }, (ctx, params) => {
        // Draw rays
        if (params.rays_progress > 0) {
            ctx.save();
            ctx.strokeStyle = '#f00';
            ctx.lineWidth = 2;
            ctx.setLineDash([5, 5]);
            rays_to_draw.forEach(ray => {
                let is_right_cam = ray.side === 'right';
                ctx.save();
                ctx.globalAlpha *= params.opacity;
                if (!is_right_cam) ctx.globalAlpha *= params.non_right_opacity;
                drawSequentialChains(ctx, [[ray.start, ray.end]], params.rays_progress);
                ctx.restore();
            });
            ctx.restore();
        }

        // Draw markers
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        markers.forEach(marker => {
            ctx.beginPath();
            ctx.arc(marker.x, marker.y, 8, 0, 2 * Math.PI);
            ctx.fillStyle = '#f00';
            ctx.fill();
            ctx.lineWidth = 2;
            ctx.strokeStyle = '#000';
            ctx.stroke();
        });
        ctx.restore();
    });

    let new_rays_to_draw = [];
    cams.filter(c => c.side === 'right').forEach(cam => {
        let origin = {
            x: cam.x + Math.cos(cam.angle) * 40,
            y: cam.y + Math.sin(cam.angle) * 40
        };
        markers.forEach(marker => {
            new_rays_to_draw.push({ start: origin, end: marker });
        });
    });

    vid.add_object('new_rays', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 2;
        ctx.setLineDash([5, 5]);
        new_rays_to_draw.forEach(ray => {
            drawSequentialChains(ctx, [[ray.start, ray.end]], 1.0);
        });
        ctx.restore();
    });

    let hub_x = body_grid.center().x;
    let hub_y = body_grid.center().y;
    let hub_box = new DiagramBox({
        text: 'USB\nHub',
        width: 100,
        height: 300,
        font_size: 20,
        position: { x: hub_x, y: hub_y }
    });

    let hub_wires = [];
    cams.filter(c => c.side === 'right').forEach(cam => {
        let start = { x: body_grid.left_center().x + 180, y: cam.y };
        let end = { x: hub_x - 50, y: cam.y };
        hub_wires.push([start, end]);
    });
    hub_wires.push([{ x: hub_x + 50, y: hub_y }, { x: comp_x - 75, y: hub_y }]);

    vid.add_object('hub_wires', { opacity: 1, progress: 0 }, (ctx, params) => {
        if (params.progress > 0) {
            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            hub_wires.forEach(wire => {
                drawSequentialChains(ctx, [wire], params.progress);
            });
            ctx.restore();
        }
    });

    vid.add_object('emoji_frames', { opacity: 1, frame_time_ms: 0 }, (ctx, params) => {
        if (params.frame_time_ms <= 0) return;

        let right_cams = cams.filter(c => c.side === 'right');

        ctx.save();
        ctx.textDrawingMode = "glyph";
        ctx.font = '64px "Noto Color Emoji"';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        right_cams.forEach((cam, i) => {
            let y = cam.y;
            let start_x = body_grid.left_center().x - 100;

            let p_start = { x: start_x, y: y };
            let p_hub_in = { x: hub_x - 50, y: y };
            let p_hub_out = { x: hub_x + 50, y: hub_y };
            let p_comp = { x: comp_x - 75, y: hub_y };

            let d1 = p_hub_in.x - p_start.x;
            let d2_dist = Math.hypot(p_hub_out.x - p_hub_in.x, p_hub_out.y - p_hub_in.y);
            let d3 = p_comp.x - p_hub_out.x;
            let D = d1 + d2_dist + d3;

            let order = i === 0 ? 0 : (i === 1 ? 2 : 1);
            for (let f = 0; f < 20; f++) {
                let start_time = f * 800 + order * (800 / 3);
                let duration = 3000;

                if (params.frame_time_ms >= start_time && params.frame_time_ms <= start_time + duration) {
                    let prog = (params.frame_time_ms - start_time) / duration;
                    let traveled = prog * D;

                    let pos_x = 0;
                    let pos_y = 0;
                    if (traveled < d1) {
                        let sub_prog = traveled / d1;
                        pos_x = lerp(p_start.x, p_hub_in.x, sub_prog);
                        pos_y = lerp(p_start.y, p_hub_in.y, sub_prog);
                    } else if (traveled < d1 + d2_dist) {
                        let sub_prog = (traveled - d1) / d2_dist;
                        pos_x = lerp(p_hub_in.x, p_hub_out.x, sub_prog);
                        pos_y = lerp(p_hub_in.y, p_hub_out.y, sub_prog);
                    } else {
                        let sub_prog = (traveled - d1 - d2_dist) / d3;
                        pos_x = lerp(p_hub_out.x, p_comp.x, sub_prog);
                        pos_y = lerp(p_hub_out.y, p_comp.y, sub_prog);
                    }

                    if (!isNaN(pos_x) && !isNaN(pos_y)) {
                        ctx.fillText('🖼', pos_x, pos_y);
                    }
                }
            }
        });

        ctx.restore();
    });

    let switch_box = new DiagramBox({
        text: 'Network\nSwitch',
        width: 100,
        height: 350,
        font_size: 20,
        background_color: '#ffccaa'
    });
    let switch_x = body_grid.center().x;
    let switch_y = body_grid.center().y;

    let proc_box = new DiagramBox({
        text: 'Image Processor\n(CPU or FPGA)',
        font_size: 16,
        width: 160,
        height: 100,
        background_color: '#ccffcc'
    });

    let switch_wires = [];
    cams.filter(c => c.side === 'right').forEach(cam => {
        let cam_y = cam.y;
        if (cam.y < cy) cam_y = cy - 150;
        else if (cam.y > cy) cam_y = cy + 150;

        let start = { x: body_grid.left_center().x + 220, y: cam_y };
        let end = { x: switch_x - 50, y: cam_y };
        switch_wires.push([start, end]);
    });
    switch_wires.push([{ x: switch_x + 50, y: switch_y }, { x: comp_x - 75, y: switch_y }]);



    vid.add_object('switch_wires', { opacity: 1, progress: 0 }, (ctx, params) => {
        if (params.progress > 0) {
            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            switch_wires.forEach(wire => {
                drawSequentialChains(ctx, [wire], params.progress);
            });
            ctx.restore();
        }
    });

    vid.add_object('smart_frames', { opacity: 1, frame_time_ms: 0 }, (ctx, params) => {
        if (params.frame_time_ms <= 0) return;

        let right_cams = cams.filter(c => c.side === 'right');

        ctx.save();
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        right_cams.forEach((cam, i) => {
            let cam_y = cam.y;
            if (cam.y < cy) cam_y = cy - 150;
            else if (cam.y > cy) cam_y = cy + 150;

            let target_x = body_grid.left_center().x + 130;
            let proc_x = target_x;
            let start_x = body_grid.left_center().x - 100;

            let p1_start = { x: start_x, y: cam_y };
            let p1_end = { x: proc_x, y: cam_y };
            let d1 = p1_end.x - p1_start.x;

            let p2_start = { x: proc_x, y: cam_y };
            let p2_end = { x: switch_x - 50, y: cam_y };
            let d2 = p2_end.x - p2_start.x;

            let p3_start = { x: switch_x + 50, y: switch_y };
            let p3_end = { x: comp_x - 75, y: switch_y };
            let d3 = p3_end.x - p3_start.x;

            let v1 = 300;
            let v2 = 300;
            let v3 = 300;

            let t1 = d1 / v1 * 1000;
            let t2 = d2 / v2 * 1000;
            let t3 = d3 / v3 * 1000;

            for (let f = 0; f < 20; f++) {
                let start_time = f * 1000;
                let current_t = params.frame_time_ms - start_time;

                if (current_t >= 0 && current_t < t1) {
                    let prog = current_t / t1;
                    let pos_x = lerp(p1_start.x, p1_end.x, prog);
                    let pos_y = lerp(p1_start.y, p1_end.y, prog);

                    ctx.save();
                    ctx.textDrawingMode = "glyph";
                    ctx.font = '64px "Noto Color Emoji"';
                    ctx.fillStyle = '#000';
                    ctx.fillText('🖼', pos_x, pos_y);
                    ctx.restore();
                }
                else if (current_t >= t1 && current_t < t1 + t2) {
                    let prog = (current_t - t1) / t2;
                    let pos_x = lerp(p2_start.x, p2_end.x, prog);
                    let pos_y = lerp(p2_start.y, p2_end.y, prog);

                    ctx.save();
                    ctx.font = '17px monospace';
                    ctx.fillStyle = '#000';
                    ctx.fillText(`(x${i + 1},y${i + 1})`, pos_x, pos_y - 15);
                    ctx.restore();
                }
                else if (current_t >= t1 + t2) {
                    let p3_start_time = t1 + t2;
                    let p3_current_t = current_t - p3_start_time;

                    if (p3_current_t >= 0 && p3_current_t < t3) {
                        let prog = p3_current_t / t3;
                        let pos_x = lerp(p3_start.x, p3_end.x, prog);
                        let pos_y = lerp(p3_start.y, p3_end.y, prog);

                        pos_y += (i - 1) * 17;
                        pos_y -= 25;

                        ctx.save();
                        ctx.font = '17px monospace';
                        ctx.fillStyle = '#000';
                        ctx.fillText(`(x${i + 1},y${i + 1})`, pos_x, pos_y);
                        ctx.restore();
                    }
                }
            }
        });

        ctx.restore();
    });

    vid.add_object('processor_boxes', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let right_cams = cams.filter(c => c.side === 'right');
        right_cams.forEach(cam => {
            let target_x = body_grid.left_center().x + 130;
            let cam_y = cam.y;
            if (cam.y < cy) cam_y = cy - 150;
            else if (cam.y > cy) cam_y = cy + 150;

            let proc_x = target_x;
            proc_box._position = { x: proc_x, y: cam_y };
            proc_box.draw(ctx);
        });
        ctx.restore();
    });

    vid.add_object('switch_box', { opacity: 0 }, (ctx, params) => {
        if (params.opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            switch_box._position = { x: switch_x, y: switch_y };
            switch_box.draw(ctx);
            ctx.restore();
        }
    });

    vid.add_object('hub', { opacity: 0 }, (ctx, params) => {
        if (params.opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            hub_box.draw(ctx);
            ctx.restore();
        }
    });

    vid.add_object('computer', { opacity: 0 }, (ctx, params) => {
        comp_box.draw(ctx);
    });

    // Timeline orchestration
    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['cameras'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['markers_and_rays'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['markers_and_rays'], t, 0.5, { rays_progress: 1 });
    t += 0.5 + pause;

    vid.add_transition(['computer'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['wires'], t, 0.5, { progress: 1 });
    t += 0.5 + pause;

    vid.add_transition(['cameras', 'wires', 'markers_and_rays'], t, 0.5, { non_right_opacity: 0 });
    vid.add_transition(['new_rays'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['wires', 'markers_and_rays', 'new_rays'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    vid.add_transition(['cameras'], t, 1.0, { cam_slide_progress: 1, cam_scale_progress: 1 });
    t += 1.0 + pause;

    vid.add_transition(['hub'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['hub_wires'], t, 0.5, { progress: 1 });
    t += 0.5 + pause;

    vid.add_transition(['emoji_frames'], t, 20, { frame_time_ms: 20000 });
    t += 20 + pause;

    // Fade out Phase 2
    vid.add_transition(['hub', 'hub_wires', 'emoji_frames'], t, 0.5, { opacity: 0 });
    vid.add_transition(['cameras'], t, 0.5, { cam_scale_progress_2: 1, cam_spread_progress: 1 });
    t += 0.5 + pause;

    // Fade in Phase 3 components
    vid.add_transition(['processor_boxes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;
    vid.add_transition(['switch_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;
    vid.add_transition(['switch_wires'], t, 0.5, { progress: 1 });
    t += 0.5 + pause;

    // Run new smart frames
    vid.add_transition(['smart_frames'], t, 20, { frame_time_ms: 20000 });
    t += 20 + pause;

    vid.set_duration(t + 1);

    return vid;
}

function part2_triangulation_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part2_triangulation");

    vid.add_object('title', { opacity: 0, text: 'Triangulation' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_y = body_grid.center().y;
    let cam_x = body_grid.left_center().x + 70;

    let cam1_pos = { x: cam_x, y: center_y - 120 };
    let cam2_pos = { x: cam_x, y: center_y + 120 };
    let marker_pos = { x: body_grid.right_center().x - 200, y: center_y };

    let cam1_box = new DiagramBox({
        text: 'Camera 1',
        width: 140,
        height: 80,
        font_size: 20,
        position: cam1_pos
    });

    let cam2_box = new DiagramBox({
        text: 'Camera 2',
        width: 140,
        height: 80,
        font_size: 20,
        position: cam2_pos
    });

    vid.add_object('marker', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.translate(marker_pos.x, marker_pos.y);
        ctx.beginPath();
        ctx.arc(0, 0, 15, 0, 2 * Math.PI);
        ctx.fillStyle = '#ddd';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.stroke();
        ctx.restore();
    });

    vid.add_object('cameras', { opacity: 0, cam1_y_offset: 0 }, (ctx, params) => {
        let draw_cam = (box, y_offset) => {
            let pos = shallow_copy(box._position);
            pos.y += y_offset;

            // Draw frustum
            ctx.save();
            ctx.translate(pos.x, pos.y);
            ctx.fillStyle = '#eee';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.moveTo(box._width / 2, 20);
            ctx.lineTo(box._width / 2 + 40, 40);
            ctx.lineTo(box._width / 2 + 40, -40);
            ctx.lineTo(box._width / 2, -20);
            ctx.closePath();
            ctx.fill();
            ctx.stroke();
            ctx.restore();

            // Draw box
            let old_pos = box._position;
            box._position = pos;
            box.draw(ctx);
            box._position = old_pos;
        };

        draw_cam(cam1_box, params.cam1_y_offset);
        draw_cam(cam2_box, 0);
    });

    let ray1_start_base = { x: cam1_pos.x + (cam1_box._width / 2), y: cam1_pos.y };
    let ray2_start_base = { x: cam2_pos.x + (cam2_box._width / 2), y: cam2_pos.y };

    let cam1_ray_angle = Math.atan2(marker_pos.y - ray1_start_base.y, marker_pos.x - ray1_start_base.x);
    let cam2_ray_angle = Math.atan2(marker_pos.y - ray2_start_base.y, marker_pos.x - ray2_start_base.x);
    let ray_length = 2000;

    vid.add_object('rays', { opacity: 1, ray1_progress: 0, ray2_progress: 0, cam1_y_offset: 0, dot_opacity: 0 }, (ctx, params) => {
        let start1 = { x: ray1_start_base.x, y: ray1_start_base.y + params.cam1_y_offset };
        let end1 = { x: start1.x + Math.cos(cam1_ray_angle) * ray_length, y: start1.y + Math.sin(cam1_ray_angle) * ray_length };

        let start2 = { x: ray2_start_base.x, y: ray2_start_base.y };
        let end2 = { x: start2.x + Math.cos(cam2_ray_angle) * ray_length, y: start2.y + Math.sin(cam2_ray_angle) * ray_length };

        ctx.save();
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 3;
        ctx.setLineDash([10, 10]);

        if (params.ray1_progress > 0) {
            drawSequentialChains(ctx, [[start1, end1]], params.ray1_progress);
        }
        if (params.ray2_progress > 0) {
            drawSequentialChains(ctx, [[start2, end2]], params.ray2_progress);
        }
        ctx.restore();

        let m1 = Math.tan(cam1_ray_angle);
        let m2 = Math.tan(cam2_ray_angle);
        let ix = (m1 * start1.x - m2 * start2.x + start2.y - start1.y) / (m1 - m2);
        let iy = m1 * (ix - start1.x) + start1.y;

        if (params.dot_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.dot_opacity;
            ctx.beginPath();
            ctx.arc(ix, iy, 10, 0, 2 * Math.PI);
            ctx.fillStyle = '#f00';
            ctx.fill();
            ctx.restore();
        }
    });

    let pause = 0.5;
    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['cameras', 'marker'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['rays'], t, 1, { ray1_progress: 1 });
    t += 1 + pause;

    vid.add_transition(['rays'], t, 1, { ray2_progress: 1 });
    t += 1 + pause;

    vid.add_transition(['rays'], t, 0.5, { dot_opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['cameras', 'rays'], t, 1, { cam1_y_offset: -50 });
    t += 1 + pause;

    vid.add_transition(['cameras', 'rays'], t, 2, { cam1_y_offset: 50 });
    t += 2 + pause;

    vid.add_transition(['cameras', 'rays'], t, 1, { cam1_y_offset: 0 });
    t += 1 + pause;

    vid.set_duration(t + 1);

    return vid;
}

export function part3_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part3_retroreflection");

    vid.add_object('title', { opacity: 0, text: 'Retroreflection' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let cx = body_grid.center().x;
    let cy = body_grid.center().y;

    let light_x = cx - 300;
    let marker_x = cx + 300;
    let marker_r = 50;

    vid.add_object('light_and_marker', { opacity: 1, light_opacity: 0, marker_opacity: 0 }, (ctx, params) => {
        if (params.light_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.light_opacity;
            ctx.textDrawingMode = "glyph";
            ctx.font = '64px "Noto Color Emoji"';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('💡', light_x, cy);
            ctx.restore();
        }

        if (params.marker_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.marker_opacity;
            ctx.beginPath();
            ctx.arc(marker_x, cy, marker_r, 0, 2 * Math.PI);
            ctx.fillStyle = '#eee';
            ctx.fill();
            ctx.strokeStyle = '#555';
            ctx.lineWidth = 2;
            ctx.stroke();
            ctx.restore();
        }
    });

    let draw_ray = (ctx, x, y, angle, opacity) => {
        ctx.save();
        ctx.globalAlpha *= opacity;
        ctx.translate(x, y);
        ctx.rotate(angle);
        ctx.beginPath();
        ctx.moveTo(-40, 0);
        ctx.lineTo(0, 0);
        ctx.lineTo(-10, -5);
        ctx.moveTo(0, 0);
        ctx.lineTo(-10, 5);
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;
        ctx.stroke();
        ctx.restore();
    };

    let y_offsets = [-90, -60, -30, -10, 10, 30, 60, 90];

    vid.add_object('rays_retro', { progress: 0, opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;

        let speed = 500;
        let time = params.progress * 2.0;

        y_offsets.forEach(y_off => {
            let ry = cy + y_off;
            let is_hit = Math.abs(y_off) < marker_r;

            if (is_hit) {
                let hit_x = marker_x - Math.sqrt(marker_r * marker_r - y_off * y_off);
                let dist_to_hit = hit_x - light_x;
                let time_to_hit = dist_to_hit / speed;

                if (time < time_to_hit) {
                    let cur_x = light_x + time * speed;
                    draw_ray(ctx, cur_x, ry, 0, params.opacity);
                } else {
                    let time_after_hit = time - time_to_hit;
                    let cur_x = hit_x - time_after_hit * speed;
                    if (cur_x > -100) {
                        draw_ray(ctx, cur_x, ry, Math.PI, params.opacity);
                    }
                }
            } else {
                let cur_x = light_x + time * speed;
                draw_ray(ctx, cur_x, ry, 0, params.opacity);
            }
        });
    });

    let wall_box = new DiagramBox({
        text: 'Wall',
        font_size: 20,
        width: 100,
        height: 300,
        background_color: '#444',
        text_color: '#fff'
    });

    vid.add_object('wall', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        wall_box._position = { x: marker_x + 50, y: cy };
        wall_box.draw(ctx);
        ctx.restore();
    });

    let base_angles = [];
    for (let i = 0; i < 8; i++) {
        base_angles.push(3 * Math.PI / 2 - (i / 7) * Math.PI);
    }
    let scatter_angles = [
        base_angles[3],
        base_angles[6],
        base_angles[1],
        base_angles[4],
        base_angles[7],
        base_angles[0],
        base_angles[5],
        base_angles[2]
    ];

    vid.add_object('rays_scatter', { progress: 0, opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;

        let speed = 500;
        let time = params.progress * 2.0;

        y_offsets.forEach((y_off, i) => {
            let ry = cy + y_off;
            let hit_x = marker_x;

            let dist_to_hit = hit_x - light_x;
            let time_to_hit = dist_to_hit / speed;

            if (time < time_to_hit) {
                let cur_x = light_x + time * speed;
                draw_ray(ctx, cur_x, ry, 0, params.opacity);
            } else {
                let time_after_hit = time - time_to_hit;
                let angle = scatter_angles[i];
                let dist_after_hit = time_after_hit * speed;
                let cur_x = hit_x + dist_after_hit * Math.cos(angle);
                let cur_y = ry + dist_after_hit * Math.sin(angle);

                draw_ray(ctx, cur_x, cur_y, angle, params.opacity);
            }
        });
    });

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['light_and_marker'], t, 0.5, { light_opacity: 1, marker_opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['rays_retro'], t, 0.2, { opacity: 1 });
    vid.add_transition(['rays_retro'], t, 2, { progress: 1 });
    t += 2 + pause;

    vid.add_transition(['rays_retro'], t, 0.1, { opacity: 0 });
    vid.add_transition(['wall'], t, 0.5, { opacity: 1 });
    vid.add_transition(['light_and_marker'], t, 0.5, { marker_opacity: 0 });
    t += 0.5 + pause;

    vid.add_transition(['rays_scatter'], t, 0.2, { opacity: 1 });
    vid.add_transition(['rays_scatter'], t, 2, { progress: 1 });
    t += 2 + pause;

    vid.set_duration(t + 1);

    return vid;
}

export function part4_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part4_filters");

    vid.add_object('title', { opacity: 0, text: 'How Cameras See' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let cx = body_grid.center().x;
    let cy = body_grid.center().y;

    let sensor_x = cx - 300;
    let bayer_x = sensor_x + 35;
    let marker_x = cx + 300;
    let marker_r = 60;

    let px_w = 30;
    let px_h = 20;
    let num_px = 12;
    let sensor_y_start = cy - (num_px * px_h) / 2;

    let y_offsets = [];
    for (let i = 0; i < num_px; i++) {
        y_offsets.push(sensor_y_start + i * px_h + px_h / 2);
    }

    let get_color = (val) => {
        let v = Math.floor(val * 255);
        return `rgb(${v},${v},${v})`;
    };

    vid.add_object('sensor_and_marker', { opacity: 0, p1_white: 0, p2_white: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        ctx.font = '24px sans-serif';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText('Camera', sensor_x, sensor_y_start - 35);
        ctx.fillText('Pixels', sensor_x, sensor_y_start - 10);

        ctx.beginPath();
        ctx.arc(marker_x, cy, marker_r, 0, 2 * Math.PI);
        ctx.fillStyle = '#ccc';
        ctx.fill();
        ctx.strokeStyle = '#888';
        ctx.lineWidth = 2;
        ctx.stroke();

        for (let i = 0; i < num_px; i++) {
            let ry = y_offsets[i];
            let is_middle = (i >= 3 && i <= 8);
            let is_red = (i % 3 === 0);

            let fill_val = 0;
            if (is_middle) {
                if (params.p1_white > 0) fill_val = params.p1_white;
                else if (params.p2_white > 0 && is_red) fill_val = params.p2_white;
            }

            ctx.fillStyle = get_color(fill_val);
            ctx.strokeStyle = '#888';
            ctx.lineWidth = 2;

            ctx.beginPath();
            ctx.rect(sensor_x - px_w / 2, ry - px_h / 2, px_w, px_h);
            ctx.fill();
            ctx.stroke();
        }
        ctx.restore();
    });

    let bayer_colors = ['#f88', '#8f8', '#88f'];

    vid.add_object('bayer_filter', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let bw = 15;
        for (let i = 0; i < num_px; i++) {
            let ry = y_offsets[i];
            ctx.fillStyle = bayer_colors[i % 3];
            ctx.strokeStyle = '#888';
            ctx.lineWidth = 1;

            ctx.beginPath();
            ctx.rect(bayer_x - bw / 2, ry - px_h / 2, bw, px_h);
            ctx.fill();
            ctx.stroke();
        }
        ctx.restore();
    });

    vid.add_object('lens_assembly', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let lens_w = 300;
        let lens_h = 260;
        ctx.fillStyle = 'rgba(200, 220, 255, 0.2)';
        ctx.strokeStyle = '#88c';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.rect(cx - lens_w / 2, cy - lens_h / 2, lens_w, lens_h);
        ctx.fill();
        ctx.stroke();

        ctx.font = '24px sans-serif';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText('Lens', cx, cy - lens_h / 2 - 10);

        ctx.restore();
    });

    let ir_x = cx - 150 + 75;

    vid.add_object('ir_filter', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let lens_h = 260;
        ctx.strokeStyle = '#0a0';
        ctx.lineWidth = 6;
        ctx.beginPath();
        ctx.moveTo(ir_x, cy - lens_h / 2);
        ctx.lineTo(ir_x, cy + lens_h / 2);
        ctx.stroke();

        ctx.fillStyle = '#0a0';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.font = '20px sans-serif';
        ctx.fillText('IR Filter', ir_x, cy - lens_h / 2 - 55);

        ctx.beginPath();
        ctx.moveTo(ir_x, cy - lens_h / 2 - 50);
        ctx.lineTo(ir_x, cy - lens_h / 2 - 20);
        ctx.stroke();

        ctx.beginPath();
        ctx.moveTo(ir_x, cy - lens_h / 2 - 10);
        ctx.lineTo(ir_x - 10, cy - lens_h / 2 - 22);
        ctx.lineTo(ir_x + 10, cy - lens_h / 2 - 22);
        ctx.fill();

        ctx.restore();
    });

    let draw_red_ray = (ctx, x, y, angle, opacity) => {
        ctx.save();
        ctx.globalAlpha *= opacity;
        ctx.translate(x, y);
        ctx.rotate(angle);
        ctx.beginPath();
        ctx.moveTo(-40, 0);
        ctx.lineTo(0, 0);
        ctx.lineTo(-10, -5);
        ctx.moveTo(0, 0);
        ctx.lineTo(-10, 5);
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 2;
        ctx.stroke();
        ctx.restore();
    };

    let make_rays = (name, phase) => {
        vid.add_object(name, { progress: 0, opacity: 0 }, (ctx, params) => {
            if (params.opacity <= 0 || params.progress <= 0) return;

            let speed = 600;
            let time = params.progress * 2.0;

            for (let i = 0; i < num_px; i++) {
                let ry = y_offsets[i];
                let is_middle = (i >= 3 && i <= 8);
                let is_red = (i % 3 === 0);

                let y_off = ry - cy;

                if (is_middle) {
                    let hit_x = marker_x - Math.sqrt(marker_r * marker_r - y_off * y_off);
                    let dist_to_hit = hit_x - sensor_x;
                    let time_to_hit = dist_to_hit / speed;

                    if (time < time_to_hit) {
                        let cur_x = sensor_x + time * speed;
                        draw_red_ray(ctx, cur_x, ry, 0, params.opacity);
                    } else {
                        let time_after_hit = time - time_to_hit;
                        let cur_x = hit_x - time_after_hit * speed;

                        let stop_x = sensor_x;
                        if (phase === 2 && !is_red) stop_x = bayer_x + 15;
                        if (phase === 3) stop_x = ir_x;

                        if (cur_x > stop_x) {
                            draw_red_ray(ctx, cur_x, ry, Math.PI, params.opacity);
                        }
                    }
                } else {
                    let cur_x = sensor_x + time * speed;
                    draw_red_ray(ctx, cur_x, ry, 0, params.opacity);
                }
            }
        });
    };

    make_rays('rays_p1', 1);
    make_rays('rays_p2', 2);
    make_rays('rays_p3', 3);

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;
    vid.add_transition(['sensor_and_marker'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['rays_p1'], t, 0.2, { opacity: 1 });
    vid.add_transition(['rays_p1'], t, 2, { progress: 1 });
    vid.add_transition(['sensor_and_marker'], t + 1.8, 0.2, { p1_white: 1 });
    t += 2 + pause;

    vid.add_transition(['rays_p1'], t, 0.2, { opacity: 0 });
    vid.add_transition(['sensor_and_marker'], t, 0.5, { p1_white: 0 });
    t += 0.5;
    vid.add_transition(['bayer_filter'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['rays_p2'], t, 0.2, { opacity: 1 });
    vid.add_transition(['rays_p2'], t, 2, { progress: 1 });
    vid.add_transition(['sensor_and_marker'], t + 1.8, 0.2, { p2_white: 1 });
    t += 2 + pause;

    vid.add_transition(['rays_p2'], t, 0.2, { opacity: 0 });
    vid.add_transition(['sensor_and_marker'], t, 0.5, { p2_white: 0 });
    t += 0.5;
    vid.add_transition(['lens_assembly'], t, 0.5, { opacity: 1 });
    t += 0.5;
    vid.add_transition(['ir_filter'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['rays_p3'], t, 0.2, { opacity: 1 });
    vid.add_transition(['rays_p3'], t, 2, { progress: 1 });
    t += 2 + pause;

    vid.set_duration(t + 1);

    return vid;
}

export function part5_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part5_board_power");

    vid.add_object('title', { opacity: 0, text: 'Board Power' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let cx = body_grid.center().x;
    let cy = body_grid.center().y;

    let box_w = 140;
    let box_h = 100;
    let spacing = 220;

    let box1 = new DiagramBox({
        text: 'PoE\nPower',
        font_size: 20,
        width: box_w,
        height: box_h,
        background_color: '#eef',
        text_color: '#000'
    });
    box1._position = { x: cx - spacing * 1.5, y: cy };

    let box2 = new DiagramBox({
        text: 'PoE\nController',
        font_size: 20,
        width: box_w,
        height: box_h,
        background_color: '#efe',
        text_color: '#000'
    });
    box2._position = { x: cx - spacing * 0.5, y: cy };

    let box3 = new DiagramBox({
        text: '5V Buck\nConverter',
        font_size: 20,
        width: box_w,
        height: box_h,
        background_color: '#fee',
        text_color: '#000'
    });
    box3._position = { x: cx + spacing * 0.5, y: cy };

    let box4 = new DiagramBox({
        text: 'Compute\nModule',
        font_size: 20,
        width: box_w,
        height: box_h,
        background_color: '#ffe',
        text_color: '#000'
    });
    box4._position = { x: cx + spacing * 1.5, y: cy };

    let draw_box_with_arrow = (ctx, box, prev_box, opacity) => {
        if (opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= opacity;
        box.draw(ctx);
        if (prev_box) {
            let from_x = prev_box._position.x + box_w / 2;
            let to_x = box._position.x - box_w / 2;
            let arrow_y = cy;

            ctx.fillStyle = '#000';
            ctx.strokeStyle = '#000';
            drawArrow(ctx, from_x, arrow_y, to_x, arrow_y, 4, 15);
        }
        ctx.restore();
    };

    vid.add_object('box1', { opacity: 0 }, (ctx, params) => {
        draw_box_with_arrow(ctx, box1, null, params.opacity);
    });

    vid.add_object('box2', { opacity: 0 }, (ctx, params) => {
        draw_box_with_arrow(ctx, box2, box1, params.opacity);
    });

    vid.add_object('box3', { opacity: 0 }, (ctx, params) => {
        draw_box_with_arrow(ctx, box3, box2, params.opacity);
    });

    vid.add_object('box4', { opacity: 0 }, (ctx, params) => {
        draw_box_with_arrow(ctx, box4, box3, params.opacity);
    });

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box1'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box2'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box3'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_object('probing_inject', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let inject_target_x = cx - 10;
        let inject_target_y = cy + 10;
        let inject_from_x = cx - 60;
        let inject_from_y = cy + 120;

        ctx.fillStyle = '#f00';
        ctx.strokeStyle = '#f00';
        drawArrow(ctx, inject_from_x, inject_from_y, inject_target_x, inject_target_y, 3, 10);

        ctx.font = '20px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        ctx.fillText('Inject Power', inject_from_x, inject_from_y + 10);
        ctx.fillText('Here', inject_from_x, inject_from_y + 35);
        ctx.restore();
    });

    vid.add_object('probing_measure', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let measure_target_x = cx + spacing + 10;
        let measure_target_y = cy + 10;
        let measure_from_x = cx + spacing + 60;
        let measure_from_y = cy + 120;

        ctx.fillStyle = '#f00';
        ctx.strokeStyle = '#f00';
        drawArrow(ctx, measure_from_x, measure_from_y, measure_target_x, measure_target_y, 3, 10);

        ctx.font = '20px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        ctx.fillText('Measure', measure_from_x, measure_from_y + 10);
        ctx.fillText('Here', measure_from_x, measure_from_y + 35);
        ctx.restore();
    });

    vid.add_transition(['box4'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['probing_inject'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['probing_measure'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.set_duration(t + 1);

    return vid;
}

export function part6_motion_video(canvas) {
    let vid = new Timeline();
    vid.set_name("part6_dealing_with_motion");

    vid.add_object('title', { opacity: 0, text: 'Dealing With Motion' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let center_y = body_grid.center().y;
    let initial_cam_x = body_grid.left_center().x + 70;

    // Markers and layout
    let cam1_pos = { x: initial_cam_x, y: center_y - 120 };
    let cam2_pos = { x: initial_cam_x, y: center_y + 120 };
    let initial_marker_pos = { x: body_grid.right_center().x - 200, y: center_y };

    let cam1_box = new DiagramBox({
        text: '',
        width: 140,
        height: 80,
        font_size: 20,
        position: shallow_copy(cam1_pos)
    });

    let cam2_box = new DiagramBox({
        text: '',
        width: 140,
        height: 80,
        font_size: 20,
        position: shallow_copy(cam2_pos)
    });

    // Object representing the moving components
    vid.add_object('scene', {
        opacity: 1, // Keep scene object active for internal opacity logic
        cam_opacity: 0,
        cam_shift_x: 0,
        marker_opacity: 0,
        marker_y_offset: 0,
        pins_opacity: 0,
        generator_opacity: 0,
        wire_progress: 0,
        pulse_progress: 0
    }, (ctx, params) => {
        // Update camera positions dynamically
        cam1_box._position.x = initial_cam_x + params.cam_shift_x;
        cam2_box._position.x = initial_cam_x + params.cam_shift_x;
        let c1_x = cam1_box._position.x;

        // Draw Cameras
        if (params.cam_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.cam_opacity;

            let draw_frustum = (box) => {
                ctx.save();
                ctx.translate(box._position.x, box._position.y);
                ctx.fillStyle = '#eee';
                ctx.strokeStyle = '#000';
                ctx.lineWidth = 2;
                ctx.beginPath();
                ctx.moveTo(box._width / 2, 20);
                ctx.lineTo(box._width / 2 + 40, 40);
                ctx.lineTo(box._width / 2 + 40, -40);
                ctx.lineTo(box._width / 2, -20);
                ctx.closePath();
                ctx.fill();
                ctx.stroke();
                ctx.restore();
            };
            draw_frustum(cam1_box);
            draw_frustum(cam2_box);

            cam1_box.draw(ctx);
            cam2_box.draw(ctx);

            // Draw TRIGGER text inside camera boxes if pins are fading in
            if (params.pins_opacity > 0) {
                ctx.globalAlpha = params.cam_opacity * params.pins_opacity;
                ctx.fillStyle = '#000';
                ctx.font = '12px monospace';
                ctx.textAlign = 'left';
                ctx.textBaseline = 'middle';
                ctx.fillText('TRIGGER', c1_x - 65, cam1_pos.y);
                ctx.fillText('TRIGGER', c1_x - 65, cam2_pos.y);
            }
            ctx.restore();
        }

        // Draw Marker
        let m_x = initial_marker_pos.x;
        let m_y = initial_marker_pos.y + params.marker_y_offset;

        if (params.marker_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.marker_opacity;
            ctx.translate(m_x, m_y);
            ctx.beginPath();
            ctx.arc(0, 0, 15, 0, 2 * Math.PI);
            ctx.fillStyle = '#ddd';
            ctx.fill();
            ctx.lineWidth = 2;
            ctx.strokeStyle = '#000';
            ctx.stroke();
            ctx.restore();
        }

        // Phase 3 Hardware
        if (params.pins_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.pins_opacity;
            ctx.fillStyle = '#aaa';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;

            let pin_w = 15;
            let pin_h = 10;
            let pin_x = c1_x - 70 - pin_w / 2;
            ctx.beginPath();
            ctx.rect(pin_x - pin_w / 2, cam1_pos.y - pin_h / 2, pin_w, pin_h);
            ctx.fill();
            ctx.stroke();

            ctx.beginPath();
            ctx.rect(pin_x - pin_w / 2, cam2_pos.y - pin_h / 2, pin_w, pin_h);
            ctx.fill();
            ctx.stroke();

            ctx.restore();
        }

        if (params.generator_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.generator_opacity;

            let gen_w = 40;
            let gen_h = 40;
            let gen_x = 50; // Aligns left edge (30) with title
            ctx.fillStyle = '#ccc';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.rect(gen_x - gen_w / 2, center_y - gen_h / 2, gen_w, gen_h);
            ctx.fill();
            ctx.stroke();

            ctx.restore();
        }

        if (params.wire_progress > 0) {
            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 3;

            let c1_x_cur = c1_x;
            let pin_tip_x = c1_x_cur - 70 - 15;
            let gen_x = 50;

            let w1_pts = [
                { x: pin_tip_x, y: cam1_pos.y },
                { x: gen_x, y: cam1_pos.y },
                { x: gen_x, y: center_y }
            ];

            let w2_pts = [
                { x: pin_tip_x, y: cam2_pos.y },
                { x: gen_x, y: cam2_pos.y },
                { x: gen_x, y: center_y }
            ];

            let draw_wire = (pts, progress) => {
                ctx.beginPath();
                ctx.moveTo(pts[0].x, pts[0].y);
                let d1 = Math.abs(pts[1].x - pts[0].x);
                let d2 = Math.abs(pts[2].y - pts[1].y);
                let total_d = d1 + d2;
                let cur_d = progress * total_d;

                if (cur_d <= d1) {
                    let ratio = cur_d / d1;
                    let x = pts[0].x + ratio * (pts[1].x - pts[0].x);
                    ctx.lineTo(x, pts[0].y);
                } else {
                    ctx.lineTo(pts[1].x, pts[1].y);
                    let cur_d2 = cur_d - d1;
                    let ratio = cur_d2 / d2;
                    let y = pts[1].y + ratio * (pts[2].y - pts[1].y);
                    ctx.lineTo(pts[1].x, y);
                }
                ctx.stroke();
            };

            draw_wire(w1_pts, params.wire_progress);
            draw_wire(w2_pts, params.wire_progress);

            ctx.restore();
        }

        if (params.pulse_progress > 0 && params.pulse_progress < 1) {
            ctx.save();
            ctx.fillStyle = '#f00';
            let c1_x_cur = c1_x;
            let pin_tip_x = c1_x_cur - 70 - 15;
            let gen_x = 50;

            let w1_pts = [
                { x: gen_x, y: center_y },
                { x: gen_x, y: cam1_pos.y },
                { x: pin_tip_x, y: cam1_pos.y }
            ];

            let w2_pts = [
                { x: gen_x, y: center_y },
                { x: gen_x, y: cam2_pos.y },
                { x: pin_tip_x, y: cam2_pos.y }
            ];

            let draw_pulse = (pts, progress) => {
                let d1 = Math.abs(pts[1].y - pts[0].y);
                let d2 = Math.abs(pts[2].x - pts[1].x);
                let total_d = d1 + d2;
                let cur_d = progress * total_d;
                let x = 0, y = 0;

                if (cur_d <= d1) {
                    let ratio = cur_d / d1;
                    x = pts[0].x;
                    y = pts[0].y + ratio * (pts[1].y - pts[0].y);
                } else {
                    let cur_d2 = cur_d - d1;
                    let ratio = cur_d2 / d2;
                    x = pts[1].x + ratio * (pts[2].x - pts[1].x);
                    y = pts[1].y;
                }
                ctx.fillRect(x - 5, y - 5, 10, 10);
            };

            draw_pulse(w1_pts, params.pulse_progress);
            draw_pulse(w2_pts, params.pulse_progress);
            ctx.restore();
        }
    });

    let top_y = -20;
    let bot_y = 20;

    vid.add_object('ray_cam1', { opacity: 0, progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 3;
        ctx.setLineDash([10, 10]);

        let start_x = initial_cam_x + 70;
        let start_y = cam1_pos.y;
        let end_x = initial_marker_pos.x;
        let end_y = center_y + top_y;

        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let angle = Math.atan2(dy, dx);
        let ext_end_x = start_x + Math.cos(angle) * 2000;
        let ext_end_y = start_y + Math.sin(angle) * 2000;

        drawSequentialChains(ctx, [[{ x: start_x, y: start_y }, { x: ext_end_x, y: ext_end_y }]], params.progress);
        ctx.restore();
    });

    vid.add_object('ray_cam2', { opacity: 0, progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 3;
        ctx.setLineDash([10, 10]);

        let start_x = initial_cam_x + 70;
        let start_y = cam2_pos.y;
        let end_x = initial_marker_pos.x;
        let end_y = center_y + bot_y;

        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let angle = Math.atan2(dy, dx);
        let ext_end_x = start_x + Math.cos(angle) * 2000;
        let ext_end_y = start_y + Math.sin(angle) * 2000;

        drawSequentialChains(ctx, [[{ x: start_x, y: start_y }, { x: ext_end_x, y: ext_end_y }]], params.progress);
        ctx.restore();
    });

    vid.add_object('wrong_point', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let start_x = initial_cam_x + 70;
        let m1 = ((center_y + top_y) - cam1_pos.y) / (initial_marker_pos.x - start_x);
        let b1 = cam1_pos.y - m1 * start_x;

        let m2 = ((center_y + bot_y) - cam2_pos.y) / (initial_marker_pos.x - start_x);
        let b2 = cam2_pos.y - m2 * start_x;

        let intersect_x = (b2 - b1) / (m1 - m2);
        let intersect_y = m1 * intersect_x + b1;

        ctx.beginPath();
        ctx.arc(intersect_x, intersect_y, 8, 0, 2 * Math.PI);
        ctx.fillStyle = '#f00';
        ctx.fill();

        let t_x = intersect_x - 100;
        let t_y = intersect_y - 100;

        ctx.fillStyle = '#f00';
        ctx.strokeStyle = '#f00';
        drawArrow(ctx, t_x + 20, t_y + 20, intersect_x - 10, intersect_y - 10, 3, 10);

        ctx.font = '20px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText('Wrong', t_x, t_y - 15);
        ctx.fillText('Point', t_x, t_y + 10);

        ctx.restore();
    });

    vid.add_object('rays_sync', { opacity: 0, progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 3;
        ctx.setLineDash([10, 10]);

        let draw_r = (start_y) => {
            let start_x = initial_cam_x + 80 + 70;
            let end_x = initial_marker_pos.x;
            let end_y = center_y;

            let dx = end_x - start_x;
            let dy = end_y - start_y;
            let angle = Math.atan2(dy, dx);
            let ext_end_x = start_x + Math.cos(angle) * 2000;
            let ext_end_y = start_y + Math.sin(angle) * 2000;

            drawSequentialChains(ctx, [[{ x: start_x, y: start_y }, { x: ext_end_x, y: ext_end_y }]], params.progress);
        };
        draw_r(cam1_pos.y);
        draw_r(cam2_pos.y);

        ctx.restore();
    });

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 0.5, { cam_opacity: 1, marker_opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 0.5, { marker_y_offset: -80 });
    t += 0.5;
    vid.add_transition(['scene'], t, 1.0, { marker_y_offset: 80 });
    t += 1.0;
    vid.add_transition(['scene'], t, 0.5, { marker_y_offset: 0 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 0.5, { marker_y_offset: top_y });
    t += 0.5 + pause;
    vid.add_transition(['ray_cam1'], t, 0.2, { opacity: 1 });
    vid.add_transition(['ray_cam1'], t, 0.5, { progress: 1 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 0.5, { marker_y_offset: bot_y });
    t += 0.5 + pause;
    vid.add_transition(['ray_cam2'], t, 0.2, { opacity: 1 });
    vid.add_transition(['ray_cam2'], t, 0.5, { progress: 1 });
    t += 0.5 + pause;

    vid.add_transition(['wrong_point'], t, 0.5, { opacity: 1 });
    t += 2.0;

    vid.add_transition(['wrong_point', 'ray_cam1', 'ray_cam2'], t, 0.5, { opacity: 0 });
    vid.add_transition(['scene'], t, 0.5, { marker_y_offset: 0 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 0.5, { cam_shift_x: 80 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 0.5, { pins_opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 0.5, { generator_opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['scene'], t, 1.0, { wire_progress: 1 });
    t += 1.0 + pause;

    vid.add_transition(['scene'], t, 1.0, { pulse_progress: 1 });
    t += 1.0;

    vid.add_transition(['rays_sync'], t, 0.2, { opacity: 1 });
    vid.add_transition(['rays_sync'], t, 0.5, { progress: 1 });
    t += 0.5 + pause;

    vid.set_duration(t + 1);

    return vid;
}

export function part6_time_video(canvas) {
    let vid = new Timeline();
    vid.set_name('part6_time_sync');

    let body_grid = slide_body_grid(canvas);
    let center_y = body_grid.center().y;

    // Layout
    let box_w = 300;
    let box_h = 400;

    // Left edge aligned to 30
    let c1_x = 30 + box_w / 2;
    let c2_x = canvas.width - 30 - box_w / 2;

    let c1_box = new DiagramBox({
        text: 'Camera 1',
        width: box_w,
        height: box_h,
        font_size: 24,
        text_offset: { x: 0, y: -box_h / 2 + 22 },
        position: { x: c1_x, y: center_y }
    });

    let c2_box = new DiagramBox({
        text: 'Camera 2',
        width: box_w,
        height: box_h,
        font_size: 24,
        text_offset: { x: 0, y: -box_h / 2 + 22 },
        position: { x: c2_x, y: center_y }
    });

    let sensor_w = 200;
    let sensor_h = 80;
    let pi_w = 240;
    let pi_h = 180;
    let time_w = 160;
    let time_h = 60;

    let c1_sensor = new DiagramBox({ text: 'Camera Sensor', width: sensor_w, height: sensor_h, font_size: 18, position: { x: c1_x, y: center_y - 110 } });
    let c2_sensor = new DiagramBox({ text: 'Camera Sensor', width: sensor_w, height: sensor_h, font_size: 18, position: { x: c2_x, y: center_y - 110 } });

    let c1_pi = new DiagramBox({ text: 'Raspberry Pi', width: pi_w, height: pi_h, font_size: 18, text_offset: { x: 0, y: -pi_h / 2 + 22 }, position: { x: c1_x, y: center_y + 80 } });
    let c2_pi = new DiagramBox({ text: 'Raspberry Pi', width: pi_w, height: pi_h, font_size: 18, text_offset: { x: 0, y: -pi_h / 2 + 22 }, position: { x: c2_x, y: center_y + 80 } });

    let c1_time = new DiagramBox({ text: 'Time = 1', width: time_w, height: time_h, font_size: 20, font_family: 'monospace', background_color: '#fff', position: { x: c1_x, y: center_y + 110 } });
    let c2_time = new DiagramBox({ text: 'Time = 68', width: time_w, height: time_h, font_size: 20, font_family: 'monospace', background_color: '#fff', position: { x: c2_x, y: center_y + 110 } });

    vid.add_object('title', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, 'Time Synchronization');
        ctx.restore();
    });

    vid.add_object('scene', {
        opacity: 0,
        time_opacity: 0,
        clock_progress: 0,
        fast_clock_progress: 0,
        sync_overlay: 0,
        sync_outline: 0
    }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        ctx.lineWidth = 1;
        c1_box.draw(ctx);
        c2_box.draw(ctx);

        // Vertical lines
        ctx.beginPath();
        ctx.moveTo(c1_x, center_y - 120 + sensor_h / 2);
        ctx.lineTo(c1_x, center_y + 80 - pi_h / 2);
        ctx.moveTo(c2_x, center_y - 120 + sensor_h / 2);
        ctx.lineTo(c2_x, center_y + 80 - pi_h / 2);
        ctx.lineWidth = 3;
        ctx.strokeStyle = '#000';
        ctx.stroke();

        ctx.lineWidth = 1;
        c1_sensor.draw(ctx);
        c2_sensor.draw(ctx);
        c1_pi.draw(ctx);
        c2_pi.draw(ctx);

        // Time boxes
        if (params.time_opacity > 0) {
            ctx.globalAlpha = params.opacity * params.time_opacity;

            // Calculate time values based on progress
            let c1_val = 1 + Math.floor(params.clock_progress * 5) + Math.floor(params.fast_clock_progress);
            let c2_val = 68 + Math.floor(params.clock_progress * 5) + Math.floor(params.fast_clock_progress);

            if (params.sync_overlay > 0) {
                // Phase 3 synced text
                c1_val = c2_val;
            }

            c1_time.set_text('Time = ' + c1_val);
            c2_time.set_text('Time = ' + c2_val);

            // Red outline for sync phase
            if (params.sync_outline > 0) {
                ctx.save();
                ctx.globalAlpha = params.opacity * params.sync_outline;
                ctx.strokeStyle = '#f00';
                ctx.lineWidth = 4;
                ctx.setLineDash([5, 5]);
                ctx.strokeRect(c1_x - time_w / 2 - 9, center_y + 110 - time_h / 2 - 9, time_w + 18, time_h + 18);
                ctx.restore();
            }

            c1_time.draw(ctx);
            c2_time.draw(ctx);

            // Phase 3 math overlay
            if (params.sync_outline > 0 && params.sync_overlay == 0) {
                ctx.save();
                ctx.fillStyle = '#f00';
                ctx.font = '20px monospace';
                ctx.textAlign = 'left';
                ctx.textBaseline = 'middle';
                ctx.fillText('+ 67', c1_x + time_w / 2 + 22, center_y + 110);
                ctx.restore();
            }
        }

        ctx.restore();
    });

    // Phase 1: Not in sync
    vid.add_object('not_in_sync', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#f00';
        ctx.strokeStyle = '#f00';
        ctx.font = '20px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        let mid_x = (c1_x + c2_x) / 2;
        let p_y = center_y + 110;

        ctx.fillText('Not in Sync', mid_x, p_y);

        // Draw horizontal arrows pointing out from text to the clocks
        // Text is roughly 120px wide
        drawArrow(ctx, mid_x - 60, p_y, c1_x + time_w / 2 + 10, p_y, 3, 10);
        drawArrow(ctx, mid_x + 60, p_y, c2_x - time_w / 2 - 10, p_y, 3, 10);

        ctx.restore();
    });

    // Phase 2: Packet Exchange
    let add_packet = (name, y_offset, is_c1_to_c2) => {
        vid.add_object(name, { opacity: 0, progress: 0, rx_opacity: 0, tx_time: 0, rx_time: 0, box_opacity: 1 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;

            let p_w = 40;
            let p_h = 30;

            let start_x = is_c1_to_c2 ? c1_x + box_w / 2 : c2_x - box_w / 2;
            let end_x = is_c1_to_c2 ? c2_x - box_w / 2 - p_w : c1_x + box_w / 2 + p_w;
            let p_y = center_y + y_offset;

            let cur_x = start_x + (end_x - start_x) * params.progress;

            if (params.box_opacity > 0 && params.progress < 1) {
                ctx.save();
                ctx.globalAlpha *= params.box_opacity;
                ctx.fillStyle = '#ffaa00';
                ctx.strokeStyle = '#000';
                ctx.lineWidth = 2;
                ctx.fillRect(cur_x - (is_c1_to_c2 ? 0 : p_w), p_y - p_h / 2, p_w, p_h);
                ctx.strokeRect(cur_x - (is_c1_to_c2 ? 0 : p_w), p_y - p_h / 2, p_w, p_h);
                ctx.restore();
            }

            ctx.fillStyle = '#ff0000ff';
            ctx.font = '16px monospace';
            ctx.textBaseline = 'bottom';

            // TX Text
            ctx.textAlign = is_c1_to_c2 ? 'left' : 'right';
            let tx_label = (is_c1_to_c2 ? 'TX1 = ' : 'TX2 = ') + Math.floor(params.tx_time);
            ctx.fillText(tx_label, start_x + (is_c1_to_c2 ? 10 : -10), p_y - 20);

            // RX Text
            if (params.rx_opacity > 0) {
                let dest_edge = is_c1_to_c2 ? c2_x - box_w / 2 : c1_x + box_w / 2;
                ctx.globalAlpha = params.opacity * params.rx_opacity;
                ctx.textAlign = is_c1_to_c2 ? 'right' : 'left';
                let rx_label = (is_c1_to_c2 ? 'RX1 = ' : 'RX2 = ') + Math.floor(params.rx_time);
                ctx.fillText(rx_label, dest_edge + (is_c1_to_c2 ? -10 : 10), p_y - 20);
            }

            ctx.restore();
        });
    };

    add_packet('packet1', -75, true);
    add_packet('packet2', 75, false);

    // Phase 4: Pulses
    vid.add_object('pulses', { opacity: 1, pulse1: 0, pulse2: 0, pulse3: 0, pulse4: 0, pulse5: 0 }, (ctx, params) => {
        let draw_dot = (prog) => {
            if (prog <= 0 || prog >= 1) return;
            let start_y = center_y + 80 - pi_h / 2;
            let end_y = center_y - 120 + sensor_h / 2;
            let cur_y = start_y + (end_y - start_y) * prog;

            ctx.fillStyle = '#f00';
            ctx.beginPath();
            ctx.arc(c1_x, cur_y, 8, 0, Math.PI * 2);
            ctx.arc(c2_x, cur_y, 8, 0, Math.PI * 2);
            ctx.fill();
        };
        draw_dot(params.pulse1);
        draw_dot(params.pulse2);
        draw_dot(params.pulse3);
        draw_dot(params.pulse4);
        draw_dot(params.pulse5);
    });

    let t = 0;
    let pause = 0.5;
    vid.add_transition(['title', 'scene'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    // Fade in clocks and start ticking (5 ticks)
    vid.add_transition(['scene'], t, 0.5, { time_opacity: 1 });
    vid.add_transition(['scene'], t, 2.0, { clock_progress: 1 });
    t += 2.0;

    vid.add_transition(['not_in_sync'], t, 0.5, { opacity: 1 });
    t += 2.0;
    vid.add_transition(['not_in_sync'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    // Packet 1
    vid.add_transition(['packet1'], t, 0.0, { tx_time: 6 });
    vid.add_transition(['packet1'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['packet1'], t, 2.0, { progress: 1 });
    vid.add_transition(['scene'], t, 2.0, { clock_progress: 1 + 3 / 5 }); // 3 ticks
    t += 2.0;

    vid.add_transition(['packet1'], t, 0.0, { rx_time: 76 });
    vid.add_transition(['packet1'], t, 0.5, { rx_opacity: 1 });
    t += 1.0 + pause;

    vid.add_transition(['packet1'], t, 0.5, { box_opacity: 0 });
    t += 0.5 + pause;

    // Delay and tick before Packet 2
    vid.add_transition(['scene'], t, 1.0, { clock_progress: 1 + 5 / 5 }); // 2 ticks
    t += 1.0 + pause;

    // Packet 2
    vid.add_transition(['packet2'], t, 0.0, { tx_time: 78 });
    vid.add_transition(['packet2'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['packet2'], t, 2.0, { progress: 1 });
    vid.add_transition(['scene'], t, 2.0, { clock_progress: 1 + 8 / 5 }); // 3 more ticks
    t += 2.0;

    vid.add_transition(['packet2'], t, 0.0, { rx_time: 14 });
    vid.add_transition(['packet2'], t, 0.5, { rx_opacity: 1 });
    t += 1.0 + pause;

    vid.add_transition(['packet2'], t, 0.5, { box_opacity: 0 });
    t += 0.5 + pause;

    // Math Sync overlay
    vid.add_transition(['scene'], t, 0.5, { sync_outline: 1 });
    t += 1.0 + pause;

    vid.add_transition(['scene'], t, 0.5, { sync_overlay: 1 });
    t += 1.0;

    vid.add_transition(['scene'], t, 0.5, { sync_outline: 0 });
    vid.add_transition(['packet1', 'packet2'], t, 0.5, { opacity: 0 }); // Fade out TX/RX times
    t += 0.5 + pause;

    // Pulses
    let current_fast_ticks = 0;

    // Tick to 90 (Currently at 81, need 9 ticks)
    vid.add_transition(['scene'], t, 0.9, { fast_clock_progress: 9 });
    t += 0.9;
    current_fast_ticks += 9;

    vid.add_transition(['pulses'], t, 0.5, { pulse1: 1 });
    t += 0.5;

    for (let i = 2; i <= 5; i++) {
        vid.add_transition(['scene'], t, 1.0, { fast_clock_progress: current_fast_ticks + 10 });
        t += 1.0;
        current_fast_ticks += 10;

        let p_name = 'pulse' + i;
        let trans = {}; trans[p_name] = 1;
        vid.add_transition(['pulses'], t, 0.5, trans);
        t += 0.5;
    }

    vid.set_duration(t + 1);

    return vid;
}

export function part6_divider_video(canvas) {
    let vid = new Timeline();
    vid.set_name('part6_pll');

    let body_grid = slide_body_grid(canvas);
    let center_y = body_grid.center().y;
    let body_h = body_grid.height();

    let y1 = center_y - body_h / 6 - 40; // 1/3rd down, shifted up to balance title padding
    let y2 = center_y + body_h / 6; // 2/3rds down

    let total_intervals = 45;
    let interval_w = canvas.width / total_intervals;
    let pulse_w = interval_w * 0.4; // 40% of interval is HIGH

    let draw_waveform = (ctx, base_y, high_indices) => {
        ctx.beginPath();
        let cur_x = 0;
        let cur_y = base_y + 40; // Low level

        ctx.moveTo(cur_x, cur_y);

        for (let i = 0; i < total_intervals; i++) {
            let start_x = i * interval_w;

            // Go to start of interval
            if (cur_x < start_x) {
                cur_x = start_x;
                ctx.lineTo(cur_x, cur_y);
            }

            if (high_indices.includes(i)) {
                // Go HIGH
                cur_y = base_y - 40;
                ctx.lineTo(cur_x, cur_y);

                // Move across HIGH
                cur_x = start_x + pulse_w;
                ctx.lineTo(cur_x, cur_y);

                // Go LOW
                cur_y = base_y + 40;
                ctx.lineTo(cur_x, cur_y);
            }
        }

        // Finish to end of screen
        ctx.lineTo(canvas.width, cur_y);
        ctx.stroke();
    };

    let pps_indices = [7, 22, 37]; // 3 pulses spaced by 15

    let div_indices = [];
    for (let i = 0; i < total_intervals; i++) {
        div_indices.push(i);
    }

    vid.add_object('title', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, 'Pulse Divider (PLL)');
        ctx.restore();
    });

    vid.add_object('grid_lines', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        ctx.strokeStyle = '#cccccc';
        ctx.lineWidth = 2;
        ctx.setLineDash([5, 5]);

        ctx.fillStyle = '#999999';
        ctx.font = '16px monospace';
        ctx.textAlign = 'right';
        ctx.textBaseline = 'bottom';

        for (let i = 0; i < pps_indices.length; i++) {
            let x = pps_indices[i] * interval_w;

            // Draw dotted line
            ctx.beginPath();
            ctx.moveTo(x, 75);
            ctx.lineTo(x, canvas.height - 15);
            ctx.stroke();

            // Draw text
            ctx.fillText((i + 1) + 's', x - 5, canvas.height - 15);
        }

        ctx.restore();
    });

    vid.add_object('pps_signal', { opacity: 0, draw_progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Draw Label
        ctx.fillStyle = '#000';
        ctx.font = '24px monospace';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText('PPS Input', 30, y1 + 60);

        // Clip the path based on draw_progress
        ctx.beginPath();
        ctx.rect(0, 0, canvas.width * params.draw_progress, canvas.height);
        ctx.clip();

        ctx.strokeStyle = '#ff0000';
        ctx.lineWidth = 4;
        draw_waveform(ctx, y1, pps_indices);

        ctx.restore();
    });

    vid.add_object('div_signal', { opacity: 0, draw_progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Draw Label
        ctx.fillStyle = '#000';
        ctx.font = '24px monospace';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText('Divided Output', 30, y2 + 60);

        // Clip the path based on draw_progress
        ctx.beginPath();
        ctx.rect(0, 0, canvas.width * params.draw_progress, canvas.height);
        ctx.clip();

        ctx.strokeStyle = '#00aa00';
        ctx.lineWidth = 4;
        draw_waveform(ctx, y2, div_indices);

        ctx.restore();
    });

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title', 'grid_lines'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // Phase 1: PPS Signal
    vid.add_transition(['pps_signal'], t, 0.5, { opacity: 1 });
    vid.add_transition(['pps_signal'], t, 3.0, { draw_progress: 1 });
    t += 3.0 + pause;

    // Phase 2: Divided Signal
    vid.add_transition(['div_signal'], t, 0.5, { opacity: 1 });
    vid.add_transition(['div_signal'], t, 3.0, { draw_progress: 1 });
    t += 3.0 + pause;

    vid.set_duration(t + 1);

    return vid;
}

export function part7_shutter(canvas) {
    let vid = new Timeline();
    vid.set_name('Camera Shutter');

    let body_grid = slide_body_grid(canvas);
    let center_y = body_grid.center().y;

    let cam_x = body_grid.left_center().x + 70;
    let cam_y = center_y;
    let marker_x = body_grid.right_center().x - 200;
    let marker_y = center_y;

    let cam_box = new DiagramBox({
        text: '',
        width: 140,
        height: 80,
        position: { x: cam_x, y: cam_y }
    });

    vid.add_object('title', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, 'Camera Shutter');
        ctx.restore();
    });

    vid.add_object('scene', { opacity: 0, shutter_open: 0, light_opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Light Frustum
        if (params.light_opacity > 0) {
            ctx.save();
            ctx.globalAlpha = params.opacity * params.light_opacity;
            ctx.fillStyle = 'rgba(255, 255, 100, 0.4)';

            let dx = canvas.width - (cam_x + 110);
            let dy = dx * 0.5;

            ctx.beginPath();
            ctx.moveTo(cam_x + 110, cam_y - 40);
            ctx.lineTo(canvas.width, cam_y - 40 - dy);
            ctx.lineTo(canvas.width, cam_y + 40 + dy);
            ctx.lineTo(cam_x + 110, cam_y + 40);
            ctx.closePath();
            ctx.fill();
            ctx.restore();
        }

        // Camera Box
        cam_box.draw(ctx);

        // Lens Frustum
        ctx.beginPath();
        ctx.moveTo(cam_x + 70, cam_y - 20);
        ctx.lineTo(cam_x + 110, cam_y - 40);
        ctx.lineTo(cam_x + 110, cam_y + 40);
        ctx.lineTo(cam_x + 70, cam_y + 20);
        ctx.closePath();
        ctx.fillStyle = '#eee';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.stroke();

        // Shutter
        let shutter_w = 4;
        let shutter_h = 90;
        let shutter_x = cam_x + 117; // 5px padding from lens edge + 2px stroke
        let cur_shutter_y = cam_y - (shutter_h * params.shutter_open);

        ctx.fillStyle = '#000';
        ctx.fillRect(shutter_x, cur_shutter_y - shutter_h / 2, shutter_w, shutter_h);

        // Marker
        ctx.save();
        ctx.translate(marker_x, marker_y);
        ctx.beginPath();
        ctx.arc(0, 0, 15, 0, 2 * Math.PI);
        ctx.fillStyle = '#ddd';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.stroke();
        ctx.restore();

        ctx.restore();
    });

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title', 'scene'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // Repeat Open/Close 3 times
    for (let i = 0; i < 3; i++) {
        // Open Shutter
        vid.add_transition(['scene'], t, 0.3, { shutter_open: 1 });
        t += 0.3;

        // Light emits instantly when open
        vid.add_transition(['scene'], t, 0.1, { light_opacity: 1 });
        t += 0.1;

        // Flash duration
        t += 0.4;

        // Close Shutter
        vid.add_transition(['scene'], t, 0.1, { light_opacity: 0 });
        vid.add_transition(['scene'], t, 0.3, { shutter_open: 0 });
        t += 0.3;

        // Wait between flashes
        t += pause;
    }

    vid.set_duration(t + 1);

    return vid;
}

export function part10_cca(canvas) {
    let vid = new Timeline();
    vid.set_name('part10_cca');

    let body_grid = slide_body_grid(canvas);
    let center_x = canvas.width / 2;
    let center_y = body_grid.center().y;

    // Configurable grid
    let grid_size = 24;
    let grid_w = 480;
    let grid_h = 480;
    let cell_size = grid_w / grid_size;

    let start_x = canvas.width - grid_w - 50; // Align right with 50px margin
    let start_y = canvas.height / 2 - grid_h / 2; // Center perfectly vertically

    // Generate circle pixels
    let c1_pixels = [];
    let c2_pixels = [];

    let add_circle = (arr, cx, cy) => {
        // Row 1 (2 pixels)
        arr.push((cy) * grid_size + (cx - 1));
        arr.push((cy) * grid_size + (cx));
        // Row 2 (4 pixels)
        arr.push((cy + 1) * grid_size + (cx - 2));
        arr.push((cy + 1) * grid_size + (cx - 1));
        arr.push((cy + 1) * grid_size + (cx));
        arr.push((cy + 1) * grid_size + (cx + 1));
        // Row 3 (4 pixels)
        arr.push((cy + 2) * grid_size + (cx - 2));
        arr.push((cy + 2) * grid_size + (cx - 1));
        arr.push((cy + 2) * grid_size + (cx));
        arr.push((cy + 2) * grid_size + (cx + 1));
        // Row 4 (2 pixels)
        arr.push((cy + 3) * grid_size + (cx - 1));
        arr.push((cy + 3) * grid_size + (cx));
    };

    // Top right
    add_circle(c1_pixels, Math.floor(grid_size * 0.75), Math.floor(grid_size * 0.2));
    // Bottom middle
    add_circle(c2_pixels, Math.floor(grid_size * 0.5), Math.floor(grid_size * 0.6));

    let all_white = [...c1_pixels, ...c2_pixels];
    let first_pixel = c1_pixels[0];
    let second_pixel = c1_pixels[1];

    vid.add_object('title', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#000000';
        ctx.font = '30px "Noto Sans"';
        ctx.fillText('Connecting', 30, 60);
        ctx.fillText('Components', 30, 100);
        ctx.restore();
    });

    vid.add_object('grid', { opacity: 0, scan_progress: 0, colored_up_to: -1, arrows_opacity: 0, simd_mode: 0, simd_box_start: 0, simd_box_len: 1, simd_box_opacity: 0, label_current: 0, final_points_opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let current_index = Math.floor(params.scan_progress);

        // Draw grid
        ctx.strokeStyle = '#444';
        ctx.lineWidth = 1;

        for (let i = 0; i < grid_size * grid_size; i++) {
            let x = i % grid_size;
            let y = Math.floor(i / grid_size);

            let px = start_x + x * cell_size;
            let py = start_y + y * cell_size;

            let is_white = all_white.includes(i);

            if (!is_white) {
                ctx.fillStyle = '#000';
            } else {
                let colored_idx = Math.floor(params.colored_up_to);
                if (i <= colored_idx || (i === current_index && params.label_current > 0)) {
                    // Passed by scanner or explicitly labeling current. Assign component color
                    if (c1_pixels.includes(i)) {
                        ctx.fillStyle = '#add8e6'; // Light blue
                    } else if (c2_pixels.includes(i)) {
                        ctx.fillStyle = '#90ee90'; // Light green
                    }
                } else {
                    ctx.fillStyle = '#fff';
                }
            }

            ctx.fillRect(px, py, cell_size, cell_size);
            ctx.strokeRect(px, py, cell_size, cell_size);
        }

        // Draw scanner red outline
        if (params.simd_mode > 0) {
            if (params.simd_box_opacity > 0) {
                let box_x = Math.floor(params.simd_box_start) % grid_size;
                let box_y = Math.floor(Math.floor(params.simd_box_start) / grid_size);

                ctx.save();
                ctx.globalAlpha *= params.simd_box_opacity;
                ctx.strokeStyle = '#f00';
                ctx.lineWidth = 3;
                ctx.strokeRect(start_x + box_x * cell_size, start_y + box_y * cell_size, cell_size * params.simd_box_len, cell_size);
                ctx.restore();
            }
        } else if (current_index >= 0 && current_index < grid_size * grid_size) {
            let cur_x = current_index % grid_size;
            let cur_y = Math.floor(current_index / grid_size);

            ctx.strokeStyle = '#f00';
            ctx.lineWidth = 3;
            ctx.strokeRect(start_x + cur_x * cell_size, start_y + cur_y * cell_size, cell_size, cell_size);

            if (params.arrows_opacity > 0) {
                ctx.save();
                ctx.globalAlpha = params.opacity * params.arrows_opacity;
                ctx.strokeStyle = '#ffff00';
                ctx.lineWidth = 2;

                let cx = start_x + (cur_x * cell_size) + cell_size / 2;
                let cy = start_y + (cur_y * cell_size) + cell_size / 2;

                let draw_neighbor_outline = (nx, ny) => {
                    // Check grid bounds
                    if (nx >= 0 && nx < grid_size && ny >= 0 && ny < grid_size) {
                        ctx.strokeRect(start_x + nx * cell_size, start_y + ny * cell_size, cell_size, cell_size);
                    }
                };

                draw_neighbor_outline(cur_x - 1, cur_y); // Left
                draw_neighbor_outline(cur_x - 1, cur_y - 1); // Top-Left
                draw_neighbor_outline(cur_x, cur_y - 1); // Top
                draw_neighbor_outline(cur_x + 1, cur_y - 1); // Top-Right

                ctx.restore();
            }
        }

        // Draw final 2D points
        if (params.final_points_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.final_points_opacity;

            let cx1 = start_x + Math.floor(grid_size * 0.75) * cell_size;
            let cy1 = start_y + (Math.floor(grid_size * 0.2) + 2) * cell_size;

            let cx2 = start_x + Math.floor(grid_size * 0.5) * cell_size;
            let cy2 = start_y + (Math.floor(grid_size * 0.6) + 2) * cell_size;

            // Red dots
            ctx.fillStyle = '#ff0000';
            ctx.beginPath();
            ctx.arc(cx1, cy1, 8, 0, Math.PI * 2);
            ctx.fill();
            ctx.beginPath();
            ctx.arc(cx2, cy2, 8, 0, Math.PI * 2);
            ctx.fill();

            // Label
            let label_x = start_x - 120;
            let label_y = start_y + grid_h / 2;

            ctx.fillStyle = '#ff0000';
            ctx.font = '28px monospace';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('Final 2D', label_x, label_y - 20);
            ctx.fillText('Points', label_x, label_y + 20);

            // Arrows
            ctx.strokeStyle = '#ff0000';
            ctx.fillStyle = '#ff0000';
            drawArrow(ctx, label_x + 80, label_y - 20, cx1 - 10, cy1, 3, 15);
            drawArrow(ctx, label_x + 80, label_y + 20, cx2 - 10, cy2, 3, 15);

            ctx.restore();
        }

        ctx.restore();
    });

    let t = 0;
    let pause = 0.5;

    vid.add_transition(['title', 'grid'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // Scan to first pixel
    vid.add_transition(['grid'], t, 2.0, { scan_progress: first_pixel, colored_up_to: first_pixel - 1 });
    t += 2.0;

    // Arrows for Pixel 1
    vid.add_transition(['grid'], t, 0.3, { arrows_opacity: 1 });
    t += 0.3 + pause;
    vid.add_transition(['grid'], t, 0.0, { label_current: 1, colored_up_to: first_pixel }); // label the pixel
    t += pause;
    vid.add_transition(['grid'], t, 0.3, { arrows_opacity: 0 });
    t += 0.3;

    // Move to Pixel 2
    vid.add_transition(['grid'], t, 0.0, { label_current: 0 });
    vid.add_transition(['grid'], t, 0.2, { scan_progress: second_pixel, colored_up_to: second_pixel - 1 });
    t += 0.2;

    // Arrows for Pixel 2
    vid.add_transition(['grid'], t, 0.3, { arrows_opacity: 1 });
    t += 0.3 + pause;
    vid.add_transition(['grid'], t, 0.0, { label_current: 1, colored_up_to: second_pixel }); // label the pixel
    t += pause;
    vid.add_transition(['grid'], t, 0.3, { arrows_opacity: 0 });
    t += 0.3;

    // Fast sweep remaining pixels
    vid.add_transition(['grid'], t, 0.0, { label_current: 0 });
    vid.add_transition(['grid'], t, 4.0, { scan_progress: grid_size * grid_size, colored_up_to: grid_size * grid_size - 1 });
    t += 4.0 + pause;

    // Final 2D Points
    vid.add_transition(['grid'], t, 0.5, { final_points_opacity: 1 });
    t += 0.5 + 2.0;

    // Reset for SIMD Phase
    vid.add_transition(['grid'], t, 0.0, { scan_progress: 0, colored_up_to: -1, simd_mode: 1, simd_box_opacity: 0, final_points_opacity: 0 });
    t += 1.0; // 1 second pause

    // Programmatic SIMD scan loop
    let simd_chunk_size = 12;
    let total_pixels = grid_size * grid_size;
    let num_chunks = total_pixels / simd_chunk_size;

    for (let c = 0; c < num_chunks; c++) {
        let start_i = c * simd_chunk_size;
        let has_white = false;

        for (let j = 0; j < simd_chunk_size; j++) {
            if (all_white.includes(start_i + j)) {
                has_white = true;
            }
        }

        if (!has_white) {
            // Empty 12-pixel block: Scan rapidly
            vid.add_transition(['grid'], t, 0.0, { simd_box_start: start_i, simd_box_len: simd_chunk_size, simd_box_opacity: 1 });
            t += 0.1; // Short pause on the 12-wide box
            vid.add_transition(['grid'], t, 0.0, { simd_box_opacity: 0, scan_progress: start_i + simd_chunk_size, colored_up_to: start_i + simd_chunk_size - 1 });
        } else {
            // White pixel detected: Fall back to slow scalar scan
            for (let j = 0; j < simd_chunk_size; j++) {
                let pixel_i = start_i + j;
                vid.add_transition(['grid'], t, 0.0, { simd_box_start: pixel_i, simd_box_len: 1, simd_box_opacity: 1 });
                t += 0.05; // Slightly slower, individual pixel checking
                vid.add_transition(['grid'], t, 0.0, { scan_progress: pixel_i + 1, colored_up_to: pixel_i });
            }
            vid.add_transition(['grid'], t, 0.0, { simd_box_opacity: 0 });
        }
    }

    t += pause;

    vid.set_duration(t + 1);

    return vid;
}

export function part10_life(canvas) {
    let vid = new Timeline();
    vid.set_name('part10_life');

    let body_grid = slide_body_grid(canvas);
    let center_x = canvas.width / 2;
    let center_y = body_grid.center().y;

    let box_w = 140;
    let box_h = 100;
    let spacing = 220;

    let b1 = new DiagramBox({
        text: 'Exposing\nImage',
        font_size: 20, width: box_w, height: box_h,
        background_color: '#eef', text_color: '#000'
    });
    let b2 = new DiagramBox({
        text: 'Internal\nTransfer',
        font_size: 20, width: box_w, height: box_h,
        background_color: '#efe', text_color: '#000'
    });
    let b3 = new DiagramBox({
        text: 'Readout',
        font_size: 20, width: box_w, height: box_h,
        background_color: '#fee', text_color: '#000'
    });
    let b4 = new DiagramBox({
        text: 'Raspberry Pi\nRAM',
        font_size: 20, width: box_w, height: box_h,
        background_color: '#ffe', text_color: '#000'
    });
    let b5 = new DiagramBox({
        text: 'Final\n2D Points',
        font_size: 20, width: box_w, height: box_h,
        background_color: '#eff', text_color: '#000'
    });

    vid.add_object('title', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, 'Life of a Pixel');
        ctx.restore();
    });

    vid.add_object('pipeline', {
        opacity: 1,
        b1_op: 0, b1_pos: 0,
        b2_op: 0, b2_pos: -1, a1_op: 0,
        b3_op: 0, b3_pos: -1, a2_op: 0,
        b4_op: 0, b4_pos: -1, a3_op: 0,
        b5_op: 0, b5_pos: -1, a4_op: 0,
        grid_op: 0, a3_target_grid: 0,
        packet_op: 0, packet_progress: 0, grid_data_op: 0,
        pipelined_progress: 0, scanner_progress: -0.01, scanner_op: 0
    }, (ctx, params) => {
        let draw_box = (box, op, pos) => {
            if (op <= 0) return;
            ctx.save();
            ctx.globalAlpha *= op;
            box._position = { x: center_x - pos * spacing, y: center_y };
            box.draw(ctx);
            ctx.restore();
        };

        // Compute grid geometry
        let grid_size = 24;
        let grid_w = 480;
        let grid_h = 480;
        let cell_size = grid_w / grid_size;
        let grid_start_x = canvas.width - grid_w - 50;
        let grid_start_y = canvas.height / 2 - grid_h / 2;

        let draw_arrow_label = (op, pos_left, pos_right, label, arrow_id) => {
            if (op <= 0) return;
            ctx.save();
            ctx.globalAlpha *= op;

            // From old box (left) to new box (right)
            let from_x = center_x - pos_left * spacing + box_w / 2;
            let to_x = center_x - pos_right * spacing - box_w / 2;

            if (arrow_id === 'a3' && params.a3_target_grid > 0) {
                // Interpolate to_x towards the grid's left edge
                let grid_edge_x = grid_start_x; // touch exactly
                to_x = to_x * (1 - params.a3_target_grid) + grid_edge_x * params.a3_target_grid;
            }

            ctx.fillStyle = '#000';
            ctx.strokeStyle = '#000';
            drawArrow(ctx, from_x, center_y, to_x, center_y, 4, 15);

            ctx.font = '20px monospace';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'bottom';
            let label_alpha = 1.0;
            if (arrow_id === 'a3') {
                label_alpha = 1.0 - (0.5 * params.a3_target_grid);
            } else if (arrow_id === 'a2') {
                label_alpha = 1.0 - (1.0 * params.a3_target_grid);
            }

            if (label_alpha > 0) {
                ctx.globalAlpha *= label_alpha;
                ctx.fillText(label, (from_x + to_x) / 2, center_y - 10);
            }
            ctx.restore();
        };

        draw_box(b1, params.b1_op, params.b1_pos);
        draw_box(b2, params.b2_op, params.b2_pos);
        draw_box(b3, params.b3_op, params.b3_pos);
        draw_box(b4, params.b4_op, params.b4_pos);
        draw_box(b5, params.b5_op, params.b5_pos);

        draw_arrow_label(params.a1_op, params.b1_pos, params.b2_pos, '250us', 'a1');
        draw_arrow_label(params.a2_op, params.b2_pos, params.b3_pos, '100us', 'a2');
        draw_arrow_label(params.a3_op, params.b3_pos, params.b4_pos, '~8ms', 'a3');
        draw_arrow_label(params.a4_op, params.b4_pos, params.b5_pos, '200us', 'a4');

        // Draw grid
        if (params.grid_op > 0) {
            let add_circle = (arr, cx, cy) => {
                arr.push((cy) * grid_size + (cx - 1));
                arr.push((cy) * grid_size + (cx));
                arr.push((cy + 1) * grid_size + (cx - 2));
                arr.push((cy + 1) * grid_size + (cx - 1));
                arr.push((cy + 1) * grid_size + (cx));
                arr.push((cy + 1) * grid_size + (cx + 1));
                arr.push((cy + 2) * grid_size + (cx - 2));
                arr.push((cy + 2) * grid_size + (cx - 1));
                arr.push((cy + 2) * grid_size + (cx));
                arr.push((cy + 2) * grid_size + (cx + 1));
                arr.push((cy + 3) * grid_size + (cx - 1));
                arr.push((cy + 3) * grid_size + (cx));
            };
            let c1_pixels = [];
            let c2_pixels = [];
            add_circle(c1_pixels, 19, 5);
            add_circle(c2_pixels, 12, 18);
            let all_white = c1_pixels.concat(c2_pixels);

            ctx.save();
            ctx.globalAlpha *= params.grid_op;

            ctx.strokeStyle = '#444';
            ctx.lineWidth = 1;

            let chunk_size = 8;
            let num_chunks = (grid_size * grid_size) / chunk_size;
            let current_progress = params.grid_data_op * num_chunks;

            for (let i = 0; i < grid_size * grid_size; i++) {
                let x = i % grid_size;
                let y = Math.floor(i / grid_size);

                let px = grid_start_x + x * cell_size;
                let py = grid_start_y + y * cell_size;

                let pixel_chunk = Math.floor(i / chunk_size);
                let pixel_op = 0;
                if (pixel_chunk < Math.floor(current_progress)) {
                    pixel_op = 1.0;
                } else if (pixel_chunk === Math.floor(current_progress)) {
                    pixel_op = current_progress - Math.floor(current_progress);
                } else {
                    pixel_op = 0.0;
                }

                let opacity_data = pixel_op;
                let opacity_red = 1.0 - pixel_op;

                // Draw base red grid
                if (opacity_red > 0) {
                    ctx.globalAlpha = params.grid_op * opacity_red;
                    ctx.fillStyle = '#ffcccc';
                    ctx.fillRect(px, py, cell_size, cell_size);
                    ctx.strokeRect(px, py, cell_size, cell_size);
                }

                // Draw data grid
                if (opacity_data > 0) {
                    ctx.globalAlpha = params.grid_op * opacity_data;

                    let scan_idx = Math.floor(params.scanner_progress * (grid_size * grid_size));
                    if (all_white.includes(i)) {
                        if (i <= scan_idx) {
                            ctx.fillStyle = c1_pixels.includes(i) ? '#add8e6' : '#90ee90';
                        } else {
                            ctx.fillStyle = '#ffffff';
                        }
                    } else {
                        ctx.fillStyle = '#000000';
                    }

                    ctx.fillRect(px, py, cell_size, cell_size);
                    ctx.strokeRect(px, py, cell_size, cell_size);
                }
            }

            // Draw scanner box
            if (params.scanner_op > 0) {
                let s_idx = Math.floor(params.scanner_progress * (grid_size * grid_size));
                let sx = s_idx % grid_size;
                let sy = Math.floor(s_idx / grid_size);
                if (s_idx < 0) { sx = -1; sy = 0; }

                ctx.globalAlpha = params.grid_op * params.scanner_op;
                ctx.strokeStyle = '#00ff00';
                ctx.lineWidth = 3;
                ctx.strokeRect(grid_start_x + sx * cell_size, grid_start_y + sy * cell_size, cell_size, cell_size);
            }

            ctx.restore();
        }

        // Draw packet
        if (params.packet_op > 0) {
            ctx.save();
            ctx.globalAlpha *= params.packet_op;

            let from_x = center_x - params.b3_pos * spacing + box_w / 2;
            let to_x = grid_start_x;

            let px = from_x + (to_x - from_x) * params.packet_progress;

            ctx.fillStyle = '#ff0000';
            ctx.beginPath();
            ctx.arc(px, center_y, 10, 0, Math.PI * 2);
            ctx.fill();

            ctx.restore();
        }

        // Draw pipelined dots
        if (params.pipelined_progress > 0 && params.pipelined_progress <= 1) {
            let t_seconds = params.pipelined_progress * 3.0;
            let from_x = center_x - params.b3_pos * spacing + box_w / 2;
            let to_x = grid_start_x;

            ctx.save();
            ctx.fillStyle = '#ff0000';
            for (let i = 0; i < 16; i++) {
                let start_t = i * 0.15;
                let end_t = start_t + 0.75;
                if (t_seconds >= start_t && t_seconds < end_t) {
                    let prog = (t_seconds - start_t) / 0.75;
                    let px = from_x + (to_x - from_x) * prog;
                    ctx.beginPath();
                    ctx.arc(px, center_y, 8, 0, Math.PI * 2); // 8px radius larger dots
                    ctx.fill();
                }
            }
            ctx.restore();
        }
    });

    let t = 0;
    let p = 0.5;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + p;

    // 1. Center Exposing
    vid.add_transition(['pipeline'], t, 0.5, { b1_op: 1 });
    t += 0.5 + p;

    // 2. Fade Arrow 1 (Points rightwards out of bounds)
    vid.add_transition(['pipeline'], t, 0.3, { a1_op: 1 });
    t += 0.3 + p;

    // 3. Slide Arrow & Box Left
    vid.add_transition(['pipeline'], t, 0.5, { b1_pos: 1, b2_pos: 0 });
    t += 0.5;

    // 4. Fade Internal Transfer
    vid.add_transition(['pipeline'], t, 0.3, { b2_op: 1 });
    t += 0.3 + p;

    // 5. Fade Arrow 2
    vid.add_transition(['pipeline'], t, 0.0, { b3_pos: -1 });
    vid.add_transition(['pipeline'], t, 0.3, { a2_op: 1 });
    t += 0.3 + p;

    // 6. Slide everything left
    vid.add_transition(['pipeline'], t, 0.5, { b1_pos: 2, b2_pos: 1, b3_pos: 0 });
    t += 0.5;

    // 7. Fade Readout
    vid.add_transition(['pipeline'], t, 0.3, { b3_op: 1 });
    t += 0.3 + p;

    // 8. Fade Arrow 3
    vid.add_transition(['pipeline'], t, 0.0, { b4_pos: -1 });
    vid.add_transition(['pipeline'], t, 0.3, { a3_op: 1 });
    t += 0.3 + p;

    // 9. Slide everything left
    vid.add_transition(['pipeline'], t, 0.5, { b1_pos: 3, b2_pos: 2, b3_pos: 1, b4_pos: 0 });
    t += 0.5;

    // 10. Fade Raspberry Pi RAM
    vid.add_transition(['pipeline'], t, 0.3, { b4_op: 1 });
    t += 0.3 + p;

    // 11. Fade Arrow 4
    vid.add_transition(['pipeline'], t, 0.0, { b5_pos: -1 });
    vid.add_transition(['pipeline'], t, 0.3, { a4_op: 1 });
    t += 0.3 + p;

    // 12. Slide everything left
    vid.add_transition(['pipeline'], t, 0.5, { b1_pos: 4, b2_pos: 3, b3_pos: 2, b4_pos: 1, b5_pos: 0 });
    t += 0.5;

    // 13. Fade Final 2D Points
    vid.add_transition(['pipeline'], t, 0.3, { b5_op: 1 });
    t += 0.3 + p;

    // 14. Combined Transition: Grid Fade In, Arrows redirect, Boxes shift right, Old boxes fade out
    vid.add_transition(['pipeline'], t, 0.5, {
        b5_op: 0, a4_op: 0,
        b4_op: 0,
        a3_target_grid: 1,
        b1_pos: 3.5, b2_pos: 2.5, b3_pos: 1.5,
        grid_op: 1
    });
    t += 0.5 + p;

    // 15. Fade in Packet
    vid.add_transition(['pipeline'], t, 0.3, { packet_op: 1 });
    t += 0.3;

    // 16. Animate Packet traveling to grid
    vid.add_transition(['pipeline'], t, 3.0, { packet_progress: 1.0 });
    t += 3.0;

    // 17. Packet hits grid: fade out packet, fade in grid data
    vid.add_transition(['pipeline'], t, 0.3, { packet_op: 0, grid_data_op: 1 });
    t += 0.3 + p;

    // 18. Fade grid back to red
    vid.add_transition(['pipeline'], t, 0.3, { grid_data_op: 0 });
    t += 0.3 + p;

    // 19. Pipelined 16-dot animation
    // The first dot arrives at exactly t=0.75s of the 3.0s animation.
    vid.add_transition(['pipeline'], t, 0.75, { pipelined_progress: 0.25, grid_data_op: 0 });
    t += 0.75;

    // The remaining dots arrive over the last 2.25s, gradually fading in the image data
    vid.add_transition(['pipeline'], t, 2.25, { pipelined_progress: 1.0, grid_data_op: 1 });
    t += 2.25 + p;

    // 20. Fade out to red quickly & reset pipelined dots
    vid.add_transition(['pipeline'], t, 0.3, { grid_data_op: 0 });
    vid.add_transition(['pipeline'], t, 0.0, { pipelined_progress: 0 });
    t += 0.3 + p;

    // 21. Fade in scanner box at -1
    vid.add_transition(['pipeline'], t, 0.3, { scanner_op: 1 });
    t += 0.3 + p;

    // 22. Pipelined fill + Trailing Scanner
    // Fill starts at t=0.75
    vid.add_transition(['pipeline'], t, 0.75, { pipelined_progress: 0.25, grid_data_op: 0 });

    // Scanner runs for exactly 2.25s but starts exactly 0.28125s AFTER fill starts
    vid.add_transition(['pipeline'], t + 0.75 + 0.28125, 2.25, { scanner_progress: 1.0 });
    t += 0.75;

    vid.add_transition(['pipeline'], t, 2.25, { pipelined_progress: 1.0, grid_data_op: 1 });
    t += 2.25 + p;

    // 23. Fade out scanner
    vid.add_transition(['pipeline'], t, 0.3, { scanner_op: 0 });
    t += 0.3 + p;

    vid.set_duration(t + 1);
    return vid;
}
