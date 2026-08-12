import { Timeline, draw_title, slide_body_grid, DiagramBox } from '../../utils.js';
import { drawArrow, drawArrowPos } from '../../arrow.js';
import { math_to_img, math_scale } from '../../mathjax.js';

export async function configure(canvas) {
    return part13_kinematics(canvas);
}

function drawHorizontalDim(ctx, x1, x2, yBase, dimY, labelText, color = '#a200ff') {
    const dir = Math.sign(dimY - yBase);
    const extY = yBase + dir * 8;

    ctx.lineWidth = 2;
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.font = '20px sans-serif';
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";

    ctx.beginPath();
    ctx.moveTo(x1, extY);
    ctx.lineTo(x1, dimY + dir * 10);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(x2, extY);
    ctx.lineTo(x2, dimY + dir * 10);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(x1, dimY);
    ctx.lineTo(x2, dimY);
    ctx.stroke();

    const midX = (x1 + x2) / 2;
    ctx.save();
    ctx.translate(midX, dimY + dir * 25);
    const textWidth = ctx.measureText(labelText).width;
    ctx.fillStyle = "#fff";
    ctx.fillRect(-textWidth / 2 - 2, -8, textWidth + 4, 16);
    ctx.fillStyle = color;
    ctx.fillText(labelText, 0, 0);
    ctx.restore();
}

export function part3_triangulation(canvas) {
    let vid = new Timeline();
    vid.set_name("part3_triangulation");

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

    vid.add_object('cameras', { opacity: 0, cam1_y_offset: 0, cam2_opacity: 1 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let draw_cam = (box, y_offset, op) => {
            if (op <= 0) return;
            ctx.save();
            ctx.globalAlpha *= op;
            let pos = box._position;

            // Draw frustum
            ctx.save();
            ctx.translate(pos.x, pos.y + y_offset);
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
            box._position = { x: old_pos.x, y: old_pos.y + y_offset };
            box.draw(ctx);
            box._position = old_pos;
            ctx.restore();
        };

        draw_cam(cam1_box, params.cam1_y_offset, 1);
        draw_cam(cam2_box, 0, params.cam2_opacity);
        ctx.restore();
    });

    vid.add_object('marker', { opacity: 0, y_offset: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.translate(marker_pos.x, marker_pos.y + params.y_offset);

        ctx.beginPath();
        ctx.arc(0, 0, 15, 0, 2 * Math.PI);
        ctx.fillStyle = '#ddd';
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = '#000';
        ctx.stroke();

        ctx.font = '20px sans-serif';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText("Marker", 0, -25);

        ctx.restore();
    });

    let ray1_start_base = { x: cam1_pos.x + (cam1_box._width / 2), y: cam1_pos.y };
    let ray2_start_base = { x: cam2_pos.x + (cam2_box._width / 2), y: cam2_pos.y };

    let cam1_ray_angle = Math.atan2(marker_pos.y - ray1_start_base.y, marker_pos.x - ray1_start_base.x);
    let cam2_ray_angle = Math.atan2(marker_pos.y - ray2_start_base.y, marker_pos.x - ray2_start_base.x);

    vid.add_object('rays', { opacity: 0, cam_line_purple_progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.lineWidth = 3;

        // Camera to camera line
        ctx.beginPath();
        ctx.moveTo(ray1_start_base.x, ray1_start_base.y);
        ctx.lineTo(ray2_start_base.x, ray2_start_base.y);
        ctx.strokeStyle = '#ff9999';
        ctx.stroke();

        if (params.cam_line_purple_progress > 0) {
            ctx.save();
            ctx.globalAlpha *= params.cam_line_purple_progress;
            ctx.beginPath();
            ctx.moveTo(ray1_start_base.x, ray1_start_base.y);
            ctx.lineTo(ray2_start_base.x, ray2_start_base.y);
            ctx.strokeStyle = '#a200ff';
            ctx.lineWidth = 6;
            ctx.stroke();
            ctx.restore();
        }

        // Rays to marker
        ctx.setLineDash([10, 10]);
        ctx.strokeStyle = '#f00';
        ctx.beginPath();
        ctx.moveTo(ray1_start_base.x, ray1_start_base.y);
        ctx.lineTo(marker_pos.x, marker_pos.y);
        ctx.stroke();

        ctx.beginPath();
        ctx.moveTo(ray2_start_base.x, ray2_start_base.y);
        ctx.lineTo(marker_pos.x, marker_pos.y);
        ctx.stroke();

        ctx.restore();
    });

    vid.add_object('xy_lines', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let mid_y = center_y;
        let p1 = { x: ray1_start_base.x, y: ray1_start_base.y };
        let p2 = { x: ray1_start_base.x, y: mid_y };
        let p3 = { x: marker_pos.x, y: mid_y };

        ctx.strokeStyle = '#a200ff';
        ctx.lineWidth = 6;
        ctx.beginPath();
        ctx.moveTo(p1.x, p1.y);
        ctx.lineTo(p2.x, p2.y);
        ctx.lineTo(p3.x, p3.y);
        ctx.stroke();

        ctx.font = '24px monospace';
        ctx.fillStyle = '#a200ff';
        ctx.textAlign = 'right';
        ctx.textBaseline = 'middle';
        ctx.fillText("Y", p1.x - 15, (p1.y + p2.y) / 2 + 20);

        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        ctx.fillText("X", (p2.x + p3.x) / 2, p2.y + 15);

        ctx.restore();
    });

    let angle1_start = cam1_ray_angle;
    let angle1_end = Math.PI / 2;
    let angle2_start = -Math.PI / 2;
    let angle2_end = cam2_ray_angle;

    vid.add_object('angles', { opacity: 0, line_width: 6 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        ctx.strokeStyle = '#a200ff';
        ctx.lineWidth = params.line_width;

        ctx.beginPath();
        ctx.arc(ray1_start_base.x, ray1_start_base.y, 40, angle1_start, angle1_end);
        ctx.stroke();

        ctx.beginPath();
        ctx.arc(ray2_start_base.x, ray2_start_base.y, 40, angle2_start, angle2_end);
        ctx.stroke();

        ctx.restore();
    });

    vid.add_object('angle_labels', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let label_x = ray1_start_base.x + 230;
        let label_y = center_y;

        ctx.font = '20px sans-serif';
        ctx.fillStyle = '#a200ff';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText("Position of marker", label_x, label_y - 12);
        ctx.fillText("in images", label_x, label_y + 12);

        let mid_angle1 = (angle1_start + angle1_end) / 2;
        let mid_angle2 = (angle2_start + angle2_end) / 2;

        let tgt1_x = ray1_start_base.x + 60 * Math.cos(mid_angle1);
        let tgt1_y = ray1_start_base.y + 60 * Math.sin(mid_angle1);

        let tgt2_x = ray2_start_base.x + 60 * Math.cos(mid_angle2);
        let tgt2_y = ray2_start_base.y + 60 * Math.sin(mid_angle2);

        ctx.strokeStyle = '#a200ff';
        ctx.fillStyle = '#a200ff';
        drawArrow(ctx, label_x - 75, label_y - 25, tgt1_x, tgt1_y, 2, 10);
        drawArrow(ctx, label_x - 75, label_y + 25, tgt2_x, tgt2_y, 2, 10);

        ctx.restore();
    });

    vid.add_object('dist_label', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let label_x = ray1_start_base.x + 40;
        let label_y = center_y;

        ctx.font = '20px sans-serif';
        ctx.fillStyle = '#a200ff';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillText("Distance between cameras", label_x, label_y);

        ctx.strokeStyle = '#a200ff';
        ctx.fillStyle = '#a200ff';
        drawArrow(ctx, label_x - 10, label_y, ray1_start_base.x + 5, label_y, 4, 10);

        ctx.restore();
    });

    vid.add_object('title2', { opacity: 0, text: 'Camera Projection' }, (ctx, params) => {
        if (params.opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            draw_title(ctx, params.text);
            ctx.restore();
        }
    });

    vid.add_object('sensor_pixels', { opacity: 0, focal_x: 120, focal_y: 0, has_focal_ray: 0, zigzag_hit: 0, tilt_angle: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let sensor_x = cam_x;
        let px_w = 20;
        let px_h = 10;
        let num_px = 16;
        let sensor_y_start = center_y - (num_px * px_h) / 2;

        ctx.translate(sensor_x, center_y);
        if (params.tilt_angle) ctx.rotate(params.tilt_angle);
        ctx.translate(-sensor_x, -center_y);

        ctx.font = '24px sans-serif';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText('Camera', sensor_x, sensor_y_start - 35);
        ctx.fillText('Sensor', sensor_x, sensor_y_start - 10);

        let hit_y = -9999;
        let intensity = 0;

        if (params.has_focal_ray > 0) {
            let marker_y = center_y + 60;
            let slope = (center_y + params.focal_y - marker_y) / (cam_x + params.focal_x - marker_pos.x);
            hit_y = marker_y + slope * ((cam_x + 10) - marker_pos.x);
            intensity = params.has_focal_ray;
        } else if (params.zigzag_hit > 0) {
            hit_y = center_y + 15;
            intensity = params.zigzag_hit;
        }

        for (let i = 0; i < num_px; i++) {
            let ry = sensor_y_start + i * px_h + px_h / 2;
            let is_hit = Math.abs(ry - hit_y) <= px_h / 2;
            let fill_val = is_hit ? Math.floor(intensity * 255) : 0;

            ctx.fillStyle = `rgb(${fill_val},${fill_val},${fill_val})`;
            ctx.strokeStyle = '#888';
            ctx.lineWidth = 1;

            ctx.beginPath();
            ctx.rect(sensor_x - px_w / 2, ry - px_h / 2, px_w, px_h);
            ctx.fill();
            ctx.stroke();
        }
        ctx.restore();
    });

    vid.add_object('lens_assembly', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let lens_x = cam_x + 150;
        let lens_w = 150;
        let lens_h = 160;

        ctx.fillStyle = 'rgba(200, 220, 255, 0.2)';
        ctx.strokeStyle = '#88c';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.rect(lens_x - lens_w / 2, center_y - lens_h / 2, lens_w, lens_h);
        ctx.fill();
        ctx.stroke();

        ctx.font = '20px sans-serif';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText('Lens', lens_x, center_y - lens_h / 2 - 10);

        ctx.restore();
    });

    vid.add_object('zigzag_ray', { opacity: 0, progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let marker_y = center_y + 60;
        let p0 = { x: marker_pos.x, y: marker_y };
        let lens_x = cam_x + 150;
        let p1 = { x: lens_x + 75, y: center_y + 30 };
        let p2 = { x: lens_x + 25, y: center_y + 65 };
        let p3 = { x: lens_x - 25, y: center_y + 10 };
        let p4 = { x: lens_x - 75, y: center_y + 40 };
        let p5 = { x: cam_x, y: center_y + 15 };

        let pts = [p0, p1, p2, p3, p4, p5];

        let total_dist = 0;
        let segs = [];
        for (let i = 0; i < pts.length - 1; i++) {
            let d = Math.hypot(pts[i + 1].x - pts[i].x, pts[i + 1].y - pts[i].y);
            segs.push({ start: pts[i], end: pts[i + 1], dist: d });
            total_dist += d;
        }

        let target_dist = params.progress * total_dist;
        let drawn_dist = 0;

        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 3;
        ctx.setLineDash([10, 10]);
        ctx.beginPath();
        ctx.moveTo(p0.x, p0.y);

        for (let i = 0; i < segs.length; i++) {
            let seg = segs[i];
            if (drawn_dist + seg.dist <= target_dist) {
                ctx.lineTo(seg.end.x, seg.end.y);
                drawn_dist += seg.dist;
            } else {
                let p = (target_dist - drawn_dist) / seg.dist;
                ctx.lineTo(
                    seg.start.x + (seg.end.x - seg.start.x) * p,
                    seg.start.y + (seg.end.y - seg.start.y) * p
                );
                break;
            }
        }
        ctx.stroke();

        ctx.restore();
    });

    vid.add_object('focal_point', { opacity: 0, focal_x: 120, focal_y: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let fx = cam_x + params.focal_x;
        let fy = center_y + params.focal_y;

        ctx.beginPath();
        ctx.arc(fx, fy, 6, 0, 2 * Math.PI);
        ctx.fillStyle = '#f00';
        ctx.fill();

        ctx.font = '22px sans-serif';
        ctx.fillStyle = '#f00';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText("Focal", fx, fy - 45);
        ctx.fillText("Point", fx, fy - 22);

        ctx.restore();
    });

    vid.add_object('focal_ray', { opacity: 0, progress: 0, focal_x: 120, focal_y: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let marker_y = center_y + 60;
        let start_p = { x: marker_pos.x, y: marker_y };
        let focal_p = { x: cam_x + params.focal_x, y: center_y + params.focal_y };
        let sensor_x = cam_x + 10;

        let dx = focal_p.x - start_p.x;
        let dy = focal_p.y - start_p.y;
        let slope = dy / dx;

        let end_y = start_p.y + slope * (sensor_x - start_p.x);
        let end_p = { x: sensor_x, y: end_y };

        let total_dist = Math.hypot(end_p.x - start_p.x, end_p.y - start_p.y);
        let draw_dist = total_dist * params.progress;

        let current_p = {
            x: start_p.x + (end_p.x - start_p.x) * (draw_dist / total_dist),
            y: start_p.y + (end_p.y - start_p.y) * (draw_dist / total_dist)
        };

        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 2;
        ctx.setLineDash([10, 10]);
        ctx.beginPath();
        ctx.moveTo(start_p.x, start_p.y);
        ctx.lineTo(current_p.x, current_p.y);
        ctx.stroke();

        ctx.restore();
    });

    vid.add_object('extent_rays', { opacity: 0, progress: 0, focal_x: 120, focal_y: 0, tilt_angle: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let focal_p = { x: cam_x + params.focal_x, y: center_y + params.focal_y };

        let cx = cam_x;
        let cy = center_y;

        let raw_top_x = cam_x + 10;
        let raw_top_y = center_y - 80;
        let raw_bot_x = cam_x + 10;
        let raw_bot_y = center_y + 80;

        let angle = params.tilt_angle || 0;

        let start_top = {
            x: cx + (raw_top_x - cx) * Math.cos(angle) - (raw_top_y - cy) * Math.sin(angle),
            y: cy + (raw_top_x - cx) * Math.sin(angle) + (raw_top_y - cy) * Math.cos(angle)
        };

        let start_bot = {
            x: cx + (raw_bot_x - cx) * Math.cos(angle) - (raw_bot_y - cy) * Math.sin(angle),
            y: cy + (raw_bot_x - cx) * Math.sin(angle) + (raw_bot_y - cy) * Math.cos(angle)
        };

        let off_x = cam_x + 800;

        let slope_top = (focal_p.y - start_top.y) / (focal_p.x - start_top.x);
        let slope_bot = (focal_p.y - start_bot.y) / (focal_p.x - start_bot.x);

        let end_top = { x: off_x, y: start_top.y + slope_top * (off_x - start_top.x) };
        let end_bot = { x: off_x, y: start_bot.y + slope_bot * (off_x - start_bot.x) };

        let p_top = {
            x: start_top.x + (end_top.x - start_top.x) * params.progress,
            y: start_top.y + (end_top.y - start_top.y) * params.progress
        };

        let p_bot = {
            x: start_bot.x + (end_bot.x - start_bot.x) * params.progress,
            y: start_bot.y + (end_bot.y - start_bot.y) * params.progress
        };

        ctx.strokeStyle = '#888';
        ctx.lineWidth = 2;
        ctx.setLineDash([5, 5]);

        ctx.beginPath();
        ctx.moveTo(start_top.x, start_top.y);
        ctx.lineTo(p_top.x, p_top.y);
        ctx.stroke();

        ctx.beginPath();
        ctx.moveTo(start_bot.x, start_bot.y);
        ctx.lineTo(p_bot.x, p_bot.y);
        ctx.stroke();

        ctx.restore();
    });

    vid.add_object('focal_length_dim', { opacity: 0, focal_x: 120 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let sensor_x = cam_x + 10;
        let focal_x_abs = cam_x + params.focal_x;

        drawHorizontalDim(ctx, sensor_x, focal_x_abs, center_y + 80, center_y + 110, "Focal Length", "#a200ff");

        ctx.restore();
    });

    vid.add_object('optical_axis', { opacity: 0, focal_y: 0, progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.progress <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let fy = center_y + params.focal_y;
        let sensor_x = cam_x + 10;
        let right_x = cam_x + 800; // extend off screen

        ctx.strokeStyle = '#a200ff';
        ctx.lineWidth = 2;

        let draw_x = sensor_x + (right_x - sensor_x) * params.progress;

        ctx.beginPath();
        ctx.moveTo(sensor_x, fy);
        ctx.lineTo(draw_x, fy);
        ctx.stroke();

        if (params.progress > 0.5) {
            let text_alpha = Math.min(1, (params.progress - 0.5) * 2);
            ctx.globalAlpha = params.opacity * text_alpha;
            ctx.font = '20px sans-serif';
            ctx.fillStyle = '#a200ff';
            ctx.textAlign = 'right';
            ctx.textBaseline = 'middle';
            ctx.fillText("Optical", sensor_x - 30, fy - 12);
            ctx.fillText("Center", sensor_x - 30, fy + 12);

            ctx.beginPath();
            ctx.arc(sensor_x, fy, 4, 0, 2 * Math.PI);
            ctx.fill();
        }

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in a "Triangulation" title and the 2 cameras and the marker
    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['cameras', 'marker'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. Fade in the two rays coming from the camera to the center of the marker
    //    Also add a light red line going between the two cameras
    vid.add_transition(['rays'], t, 0.5, { opacity: 1 });
    vid.add_transition(['cameras'], t, 0.5, { opacity: 0.5 });
    t += 0.5 + pause;

    // 3. Fade in two lines to represent the x and y offset of the marker from the top camera.
    vid.add_transition(['xy_lines'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 4. Then fade out the purple lines and labels.
    vid.add_transition(['xy_lines'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    // 5. Fade in some cad style angle markers (arcs)
    //    Also fade in a label in the middle with arrows
    vid.add_transition(['angles', 'angle_labels'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 2.0;

    // 6. Now fade out the angle label and arrows.
    vid.add_transition(['angle_labels'], t, 0.5, { opacity: 0 });
    vid.add_transition(['angles'], t, 0.5, { line_width: 2 });
    t += 0.5 + pause;

    // 7. Now fade in a label saying "Distance between cameras" with an arrow
    //    Simultaneously replace the line between the cameras with a purple one
    vid.add_transition(['dist_label'], t, 0.5, { opacity: 1 });
    vid.add_transition(['rays'], t, 0.5, { cam_line_purple_progress: 1 });
    t += 0.5 + pause + 2.0;

    // 8. Big transition: Camera Projection
    vid.add_transition(['title'], t, 0.5, { opacity: 0 });
    vid.add_transition(['title2'], t, 0.5, { opacity: 1 });

    vid.add_transition(['rays', 'xy_lines', 'angles', 'angle_labels', 'dist_label'], t, 0.5, { opacity: 0 });

    vid.add_transition(['cameras'], t, 1.0, { opacity: 1, cam1_y_offset: 120, cam2_opacity: 0 });
    vid.add_transition(['marker'], t, 1.0, { y_offset: 60 });
    t += 1.0 + pause + 1.0;

    // 9. Fade out the camera and swap in a pixel array and "Lens"
    vid.add_transition(['cameras'], t, 0.5, { opacity: 0 });
    vid.add_transition(['sensor_pixels', 'lens_assembly'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 10. Draw red dotted ray from marker to sensor through lens
    vid.add_transition(['zigzag_ray'], t, 2.0, { progress: 1 });
    vid.add_transition(['zigzag_ray'], t, 0.5, { opacity: 1 }); // Ensure opacity is 1
    t += 2.0;
    vid.add_transition(['sensor_pixels'], t, 0.2, { zigzag_hit: 1 });
    t += pause + 1.0;

    // 11. Fade out lens and zigzag ray, fade in focal point
    vid.add_transition(['lens_assembly', 'zigzag_ray'], t, 0.5, { opacity: 0 });
    vid.add_transition(['focal_point'], t, 0.5, { opacity: 1 });
    vid.add_transition(['sensor_pixels'], t, 0.5, { zigzag_hit: 0 });
    t += 0.5 + pause;

    // 12. Draw focal ray (from marker through focal point to sensor)
    vid.add_transition(['focal_ray'], t, 1.0, { progress: 1, opacity: 1 });
    t += 1.0;
    vid.add_transition(['sensor_pixels'], t, 0.2, { has_focal_ray: 1 });
    t += pause;

    // 13. Draw extent rays (from sensor corners through focal point to offscreen)
    vid.add_transition(['extent_rays'], t, 1.0, { progress: 1, opacity: 1 });
    t += 1.0 + pause;

    // 14. Animate focal point moving
    // Close to sensor
    vid.add_transition(['focal_point', 'focal_ray', 'extent_rays', 'focal_length_dim', 'sensor_pixels'], t, 1.0, { focal_x: 60 });
    t += 1.0 + pause;
    // Further away (Keep it here)
    vid.add_transition(['focal_point', 'focal_ray', 'extent_rays', 'focal_length_dim', 'sensor_pixels'], t, 1.0, { focal_x: 200 });
    t += 1.0 + pause;

    // 15. Fade in Focal Length dimension
    vid.add_transition(['focal_length_dim'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    t += 1.0;

    // 16. Draw optical axis
    vid.add_transition(['optical_axis'], t, 1.0, { progress: 1, opacity: 1 });
    t += 1.0 + pause;

    // 17. Animate focal point moving up and down
    let animate_group = ['focal_point', 'focal_ray', 'extent_rays', 'sensor_pixels', 'optical_axis'];
    vid.add_transition(animate_group, t, 1.0, { focal_y: -40 });
    t += 1.0 + pause;
    vid.add_transition(animate_group, t, 1.0, { focal_y: 40 });
    t += 1.0 + pause;
    vid.add_transition(animate_group, t, 1.0, { focal_y: 0 });
    t += 1.0 + pause;

    // 18. Animate sensor tilting
    let tilt_group = ['sensor_pixels', 'extent_rays'];
    vid.add_transition(tilt_group, t, 0.5, { tilt_angle: 0.1 });
    t += 0.5 + pause;
    vid.add_transition(tilt_group, t, 1.0, { tilt_angle: -0.1 });
    t += 1.0 + pause;
    vid.add_transition(tilt_group, t, 0.5, { tilt_angle: 0 });
    t += 0.5 + pause + 2.0;

    vid.set_duration(t + 1);

    return vid;
}

export function part3_edge_gradient(canvas) {
    let vid = new Timeline();
    vid.set_name("part3_edge_gradient");

    let body_grid = slide_body_grid(canvas);
    let center = body_grid.center();
    let px_w = 160;
    let px_h = 160;

    vid.add_object('title', { opacity: 0, text: 'Finding the Edge' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    vid.add_object('pixels', { opacity: 0, p1_val: 1, p2_val: 1, p3_val: 1 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let vals = [params.p1_val, params.p2_val, params.p3_val];

        for (let i = 0; i < 3; i++) {
            let x = center.x + (i - 1) * px_w;
            let val = Math.max(0, Math.min(1, vals[i]));
            let col = Math.floor(val * 255);

            ctx.fillStyle = `rgb(${col}, ${col}, ${col})`;
            ctx.fillRect(x - px_w / 2, center.y - px_h / 2, px_w, px_h);
        }

        ctx.strokeStyle = '#888';
        ctx.lineWidth = 1;
        for (let i = 0; i < 3; i++) {
            let x = center.x + (i - 1) * px_w;
            ctx.strokeRect(x - px_w / 2, center.y - px_h / 2, px_w, px_h);
        }

        ctx.restore();
    });

    vid.add_object('red_edge', { opacity: 0, offset_x: -px_w / 2 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let x = center.x + params.offset_x;
        let y1 = center.y - px_h / 2 - 40;
        let y2 = center.y + px_h / 2 + 40;

        ctx.beginPath();
        ctx.moveTo(x, y1);
        ctx.lineTo(x, y2);
        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 6;
        ctx.lineCap = 'round';
        ctx.stroke();

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and the 3 white pixels
    vid.add_transition(['title', 'pixels'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. Fade in vertical red line between pixel 1 and 2
    vid.add_transition(['red_edge'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 0.5;

    // 3. Fade pixels 2 and 3 to black
    vid.add_transition(['pixels'], t, 0.5, { p2_val: 0, p3_val: 0 });
    t += 0.5 + pause + 0.5;

    // 4. Animate red line gradually moving right while pixel 2 linearly moves to white
    vid.add_transition(['red_edge'], t, 2.5, { offset_x: px_w / 2 });
    vid.add_transition(['pixels'], t, 2.5, { p2_val: 1 });
    t += 2.5 + pause + 2.0;

    vid.set_duration(t + 1);

    return vid;
}

export function part3_projection(canvas) {
    let vid = new Timeline();
    vid.set_name("part3_projection");

    let body_grid = slide_body_grid(canvas);
    let grid = body_grid.split(2, 1);
    let row0_y = grid.cell(0, 0).center().y;
    let row1_y = grid.cell(1, 0).center().y;

    let x_left = body_grid.left_center().x + 90;
    let x_right = body_grid.right_center().x - 90;
    let center_x = (x_left + x_right) / 2;

    vid.add_object('title', { opacity: 0, text: 'Projection' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let boxes_data = [
        { name: 'box_0', text: "3D Points\n(40mm Grid)", width: 180, height: 90, font_size: 20, y_offset: 0 },
        { name: 'box_1', text: "Move", width: 100, height: 55, font_size: 16, y_offset: -45 },
        { name: 'box_2', text: "Divide\nBy z", width: 105, height: 55, font_size: 16, y_offset: 45 },
        { name: 'box_3', text: "Distort", width: 105, height: 55, font_size: 16, y_offset: -45 },
        { name: 'box_4', text: "× Focal\nLength", width: 110, height: 55, font_size: 15, y_offset: 45 },
        { name: 'box_5', text: "+ Center", width: 105, height: 55, font_size: 16, y_offset: -45 },
        { name: 'box_6', text: "2D Points\n(Corners)", width: 180, height: 90, font_size: 20, y_offset: 0 }
    ];

    let inter_start_x = x_left + 90 + 35 + 50;
    let inter_end_x = x_right - 90 - 35 - 50;

    let diagram_boxes = [];

    boxes_data.forEach((data, i) => {
        let cx;
        if (i === 0) {
            cx = x_left;
        } else if (i === 6) {
            cx = x_right;
        } else {
            cx = inter_start_x + ((i - 1) / 4) * (inter_end_x - inter_start_x);
        }
        let cy = row0_y + data.y_offset;

        let box = new DiagramBox({
            text: data.text,
            width: data.width,
            height: data.height,
            font_size: data.font_size,
            position: { x: cx, y: cy }
        });
        diagram_boxes.push(box);

        vid.add_object(data.name, { opacity: 0 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            box.draw(ctx);
            ctx.restore();
        });
    });

    for (let i = 0; i < 6; i++) {
        let arrow_name = `arrow_${i}_${i + 1}`;
        let from_box = diagram_boxes[i];
        let to_box = diagram_boxes[i + 1];
        let is_intermediate_src = i >= 1;

        vid.add_object(arrow_name, { opacity: 0 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            ctx.fillStyle = '#000';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;

            if (is_intermediate_src) {
                let from_pt = boxes_data[i].y_offset < 0 ? from_box.bottom_center() : from_box.top_center();
                let to_pt = to_box.left_center();
                let corner_pt = { x: from_pt.x, y: to_pt.y };

                ctx.beginPath();
                ctx.moveTo(from_pt.x, from_pt.y);
                ctx.lineTo(corner_pt.x, corner_pt.y);
                ctx.stroke();

                drawArrowPos(ctx, corner_pt, to_pt, 2, 15, false);
            } else {
                let from_pt = from_box.right_center();
                let to_pt = to_box.left_center();
                let mid_x = (from_pt.x + to_pt.x) / 2;

                ctx.beginPath();
                ctx.moveTo(from_pt.x, from_pt.y);
                ctx.lineTo(mid_x, from_pt.y);
                ctx.lineTo(mid_x, to_pt.y);
                ctx.stroke();

                drawArrowPos(ctx, { x: mid_x, y: to_pt.y }, to_pt, 2, 15, false);
            }
            ctx.restore();
        });
    }

    let proj_math = new DiagramBox({
        text: "Projection Math",
        width: 280,
        height: 90,
        font_size: 24,
        background_color: '#000',
        text_color: '#fff',
        stroke_color: '#888',
        position: { x: center_x, y: row0_y }
    });

    vid.add_object('proj_math', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        proj_math.draw(ctx);
        ctx.restore();
    });

    let from_0_pt = diagram_boxes[0].right_center();
    let to_proj_pt = proj_math.left_center();
    vid.add_object('arrow_0_proj', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        drawArrowPos(ctx, from_0_pt, to_proj_pt, 2, 15, false);
        ctx.restore();
    });

    let from_proj_pt = proj_math.right_center();
    let to_6_pt = diagram_boxes[6].left_center();
    vid.add_object('arrow_proj_6', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        drawArrowPos(ctx, from_proj_pt, to_6_pt, 2, 15, false);
        ctx.restore();
    });

    let camera_params = new DiagramBox({
        text: "Camera Parameters\n(focal length, etc.)",
        width: 320,
        height: 100,
        font_size: 22,
        position: { x: center_x, y: row1_y }
    });

    vid.add_object('camera_params', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        camera_params.draw(ctx);
        ctx.restore();
    });

    let from_cam_pt = camera_params.top_center();
    let to_proj_bottom = proj_math.bottom_center();
    vid.add_object('arrow_cam_proj', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        drawArrowPos(ctx, from_cam_pt, to_proj_bottom, 2, 15, false);
        ctx.restore();
    });

    for (let i = 0; i < 10; i++) {
        vid.add_object(`stream_box_${i}`, { opacity: 0, progress: 0 }, (ctx, params) => {
            if (params.opacity <= 0 || params.progress <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;

            let sx = diagram_boxes[0].right_center().x;
            let sy = diagram_boxes[0].right_center().y;
            let ex = diagram_boxes[6].left_center().x;
            let ey = diagram_boxes[6].left_center().y;

            let cur_x = sx + (ex - sx) * params.progress;
            let cur_y = sy + (ey - sy) * params.progress;

            if (cur_x >= center_x) {
                ctx.fillStyle = '#0c0';
                ctx.strokeStyle = '#060';
            } else {
                ctx.fillStyle = '#f00';
                ctx.strokeStyle = '#800';
            }
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.arc(cur_x, cur_y, 12, 0, Math.PI * 2);
            ctx.fill();
            ctx.stroke();

            ctx.restore();
        });
    }

    let pause = 0.5;
    let t = 0;

    // 1. Fade in Title
    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. In first row, last column, fade in 2D Points
    vid.add_transition(['box_6'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 3. In first row, first column, fade in 3D Points
    vid.add_transition(['box_0'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 4. Animate chain in one after another
    vid.add_transition(['box_1', 'arrow_0_1'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_2', 'arrow_1_2'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_3', 'arrow_2_3'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_4', 'arrow_3_4'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_5', 'arrow_4_5', 'arrow_5_6'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 5. Fade out intermediate boxes and replace with black "Projection Math" box
    let intermediate = [
        'box_1', 'box_2', 'box_3', 'box_4', 'box_5',
        'arrow_0_1', 'arrow_1_2', 'arrow_2_3', 'arrow_3_4', 'arrow_4_5', 'arrow_5_6'
    ];
    vid.add_transition(intermediate, t, 0.5, { opacity: 0 });
    vid.add_transition(['proj_math', 'arrow_0_proj', 'arrow_proj_6'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 6. Fade in "Camera Parameters" in second row below black box with arrow pointing to it
    vid.add_transition(['camera_params', 'arrow_cam_proj'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 7. Animate around 10 red boxes streaming from 3D Points to 2D Points
    for (let i = 0; i < 10; i++) {
        let st = t + i * 0.25;
        vid.add_transition([`stream_box_${i}`], st, 0.1, { opacity: 1 });
        vid.add_transition([`stream_box_${i}`], st, 1.6, { progress: 1 });
        vid.add_transition([`stream_box_${i}`], st + 1.5, 0.1, { opacity: 0 });
    }
    t += 9 * 0.25 + 1.6 + pause + 1.0;

    // Bring main boxes to top so streaming boxes appear to glide underneath/between them
    vid.move_to_top('box_0');
    vid.move_to_top('box_6');
    vid.move_to_top('proj_math');
    vid.move_to_top('camera_params');
    vid.move_to_top('arrow_cam_proj');

    vid.set_duration(t + 1);

    return vid;
}

function part3_optimization(canvas) {
    let vid = new Timeline();

    vid.set_name("part3_optimization");

    vid.add_object('title', { opacity: 0, text: 'Parameter Optimization' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let origin_x = body_grid.bottom_left().x;
    let origin_y = body_grid.bottom_left().y - 15;
    let graph_width = body_grid.width() - 20;
    let graph_height = body_grid.height() - 40;

    let get_pt = (x, y) => {
        let px = origin_x + x * graph_width + 3;
        let py = origin_y - y * graph_height - 3;
        return { x: px, y: py };
    };

    let curve_func = (x) => {
        return 0.45 + 0.28 * Math.cos(Math.PI * x) + 0.16 * Math.cos(4 * Math.PI * x);
    };

    vid.add_object('axes', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#666';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(origin_x, origin_y);
        ctx.lineTo(origin_x + graph_width, origin_y);
        ctx.moveTo(origin_x + 1.5, origin_y + 1.5);
        ctx.lineTo(origin_x + 1.5, origin_y - graph_height);
        ctx.stroke();
        ctx.restore();
    });

    vid.add_object('x_label', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.font = '26px "Noto Sans Mono"';
        ctx.fillStyle = '#444';
        ctx.textAlign = 'right';
        ctx.textBaseline = 'bottom';
        ctx.fillText("Focal Length", origin_x + graph_width - 10, origin_y - 6);
        ctx.restore();
    });

    vid.add_object('y_label', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.font = '26px "Noto Sans Mono"';
        ctx.fillStyle = '#444';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText("Error", origin_x + 15, origin_y - graph_height + 5);
        ctx.restore();
    });

    vid.add_object('curve', { opacity: 0, max_t: 0 }, (ctx, params) => {
        if (params.opacity <= 0 || params.max_t <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.lineWidth = 3;
        ctx.strokeStyle = '#0bf';
        ctx.beginPath();
        let num_steps = 400;
        let max_steps = Math.floor(num_steps * params.max_t);
        for (let i = 0; i <= max_steps; i++) {
            let x = i / num_steps;
            let y = curve_func(x);
            let pt = get_pt(x, y);
            if (i === 0) {
                ctx.moveTo(pt.x, pt.y);
            } else {
                ctx.lineTo(pt.x, pt.y);
            }
        }
        ctx.stroke();
        ctx.restore();
    });

    vid.add_object('initial_points', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#f00';
        for (let i = 0; i < 10; i++) {
            if (i === 6) continue; // Drawn by opt_dot
            let x = 0.05 + i * 0.1;
            let y = curve_func(x);
            let pt = get_pt(x, y);
            ctx.beginPath();
            ctx.arc(pt.x, pt.y, 7, 0, Math.PI * 2);
            ctx.fill();
        }
        ctx.restore();
    });

    vid.add_object('best_guess_line', { opacity: 0, x: 0.65 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        let pt = get_pt(params.x, curve_func(params.x));

        ctx.setLineDash([5, 5]);
        ctx.strokeStyle = '#888';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(pt.x, origin_y);
        ctx.lineTo(pt.x, origin_y - graph_height + 20);
        ctx.stroke();

        ctx.font = '20px "Noto Sans"';
        ctx.fillStyle = '#666';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'bottom';
        ctx.fillText("Best Guess", pt.x + 10, pt.y - 15);
        ctx.restore();
    });

    ['green_left', 'green_right'].forEach((name) => {
        vid.add_object(name, { opacity: 0, x: 0.65 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            ctx.fillStyle = '#0a0';
            let pt = get_pt(params.x, curve_func(params.x));
            ctx.beginPath();
            ctx.arc(pt.x, pt.y, 7, 0, Math.PI * 2);
            ctx.fill();
            ctx.restore();
        });
    });

    vid.add_object('opt_dot', { opacity: 0, x: 0.65 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#f00';
        let pt = get_pt(params.x, curve_func(params.x));
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, 7, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in Title & Axes
    vid.add_transition(['title', 'axes'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. Fade in X axis label
    vid.add_transition(['x_label'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 3. Fade in Y axis label and draw curve from left to right
    vid.add_transition(['y_label', 'curve'], t, 0.5, { opacity: 1 });
    vid.add_transition(['curve'], t, 2.0, { max_t: 1 });
    t += 2.0 + pause;

    // 4. Fade curve to 50% opacity
    vid.add_transition(['curve'], t, 0.5, { opacity: 0.5 });
    t += 0.5 + pause;

    // 5. Fade in 10 uniform points
    vid.add_transition(['initial_points', 'opt_dot'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 0.5;

    // 6. Fade out all but the point slightly left of global min (index 6, x=0.65) and fade in best guess line
    vid.add_transition(['initial_points'], t, 0.5, { opacity: 0 });
    vid.add_transition(['best_guess_line'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 7. Stepwise convergence to Global Minimum
    let global_steps = [0.65, 0.675, 0.695, 0.712, 0.728, 0.742, 0.754, 0.764, 0.771, 0.774, 0.775];
    for (let k = 0; k < global_steps.length - 1; k++) {
        let cur_x = global_steps[k];
        let next_x = global_steps[k + 1];
        let delta = Math.abs(next_x - cur_x);

        let left_x = cur_x - delta;
        let right_x = next_x;

        vid.add_transition(['green_left'], t, 0.01, { x: left_x });
        vid.add_transition(['green_right'], t, 0.01, { x: right_x });
        vid.add_transition(['green_left', 'green_right'], t, 0.15, { opacity: 1 });
        t += 0.2;

        vid.add_transition(['opt_dot', 'best_guess_line'], t, 0.2, { x: next_x });
        t += 0.25;

        vid.add_transition(['green_left', 'green_right'], t, 0.1, { opacity: 0 });
        t += 0.15;
    }
    t += pause + 0.5;

    // 8. Move guess dot slightly to the right of local minimum (x = 0.38)
    let local_steps = [0.38, 0.355, 0.335, 0.318, 0.303, 0.290, 0.283, 0.278, 0.276, 0.275];
    vid.add_transition(['opt_dot', 'best_guess_line'], t, 1.0, { x: local_steps[0] });
    t += 1.0 + pause + 0.5;

    // 9. Stepwise convergence to Local Minimum
    for (let k = 0; k < local_steps.length - 1; k++) {
        let cur_x = local_steps[k];
        let next_x = local_steps[k + 1];
        let delta = Math.abs(next_x - cur_x);

        let left_x = next_x;
        let right_x = cur_x + delta;

        vid.add_transition(['green_left'], t, 0.01, { x: left_x });
        vid.add_transition(['green_right'], t, 0.01, { x: right_x });
        vid.add_transition(['green_left', 'green_right'], t, 0.15, { opacity: 1 });
        t += 0.2;

        vid.add_transition(['opt_dot', 'best_guess_line'], t, 0.2, { x: next_x });
        t += 0.25;

        vid.add_transition(['green_left', 'green_right'], t, 0.1, { opacity: 0 });
        t += 0.15;
    }
    t += pause + 1.0;

    vid.move_to_top('opt_dot');

    vid.set_duration(t);
    return vid;
}

function part3_summary(canvas) {
    let vid = new Timeline();

    vid.set_name("part3_summary");

    vid.add_object('title', { opacity: 0, text: 'Everything in this video' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let grid = slide_body_grid(canvas).split(3, 3);

    let boxes = {
        box_unknown: new DiagramBox({
            text: "Unknown\nVariables",
            width: 175,
            height: 85,
            font_size: 22,
            position: grid.cell(1, 0).center()
        }),
        box_guess: new DiagramBox({
            text: "Guess\nValues",
            width: 175,
            height: 85,
            font_size: 22,
            position: grid.cell(1, 1).center()
        }),
        box_data: new DiagramBox({
            text: "Data",
            width: 175,
            height: 85,
            font_size: 22,
            position: grid.cell(0, 2).center()
        }),
        box_check: new DiagramBox({
            text: "Check",
            width: 175,
            height: 85,
            font_size: 22,
            position: grid.cell(1, 2).center()
        }),
        box_tweak: new DiagramBox({
            text: "Tweak\nGuess",
            width: 175,
            height: 85,
            font_size: 22,
            position: grid.cell(2, 2).center()
        })
    };

    Object.entries(boxes).forEach(([name, box]) => {
        vid.add_object(name, { opacity: 0 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            box.draw(ctx);
            ctx.restore();
        });
    });

    let arrows = [
        { name: 'arrow_unk_guess', from: boxes.box_unknown.right_center(), to: boxes.box_guess.left_center() },
        { name: 'arrow_guess_check', from: boxes.box_guess.right_center(), to: boxes.box_check.left_center() },
        { name: 'arrow_data_check', from: boxes.box_data.bottom_center(), to: boxes.box_check.top_center() },
        { name: 'arrow_check_tweak', from: boxes.box_check.bottom_center(), to: boxes.box_tweak.top_center() }
    ];

    arrows.forEach(({ name, from, to }) => {
        vid.add_object(name, { opacity: 0 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            ctx.fillStyle = '#000';
            ctx.strokeStyle = '#000';
            drawArrowPos(ctx, from, to, 2, 18, false);
            ctx.restore();
        });
    });

    vid.add_object('arrow_tweak_guess', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#000';
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;

        let from_pt = boxes.box_tweak.left_center();
        let to_pt = boxes.box_guess.bottom_center();
        let corner_pt = { x: to_pt.x, y: from_pt.y };

        ctx.beginPath();
        ctx.moveTo(from_pt.x, from_pt.y);
        ctx.lineTo(corner_pt.x, corner_pt.y);
        ctx.stroke();

        drawArrowPos(ctx, corner_pt, to_pt, 2, 18, false);
        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_unknown'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_guess', 'arrow_unk_guess'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_data', 'box_check', 'arrow_guess_check', 'arrow_data_check'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    vid.add_transition(['box_tweak', 'arrow_check_tweak', 'arrow_tweak_guess'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 1.0;

    vid.set_duration(t);
    return vid;
}

function part3_distortion(canvas) {
    let vid = new Timeline();

    vid.set_name("part3_distortion");

    vid.add_object('title', { opacity: 0, text: 'Distortion Squishing' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let center_x = body_grid.center().x;
    let center_y = body_grid.center().y - 15;
    let scale = 700;

    let frame_w = 820;
    let frame_h = 330;
    let frame_left = center_x - frame_w / 2;
    let frame_top = center_y - frame_h / 2;

    // Use exact fisheye rational radial distortion for intense, monotonic squishing at edges without folding back
    let distort = (x, y) => {
        let r = Math.sqrt(x * x + y * y);
        if (r < 0.0001) return { x, y };
        let k = 2.0; // High distortion intensity to make squishing visually dramatic and obvious
        let r_d = Math.atan(k * r) / k;
        let factor = r_d / r;
        return {
            x: x * factor,
            y: y * factor
        };
    };

    let to_pix = (pt) => {
        return {
            x: center_x + pt.x * scale,
            y: center_y + pt.y * scale
        };
    };

    // Draw barrel distortion line grid inside a rectangular pseudo-frame
    vid.add_object('grid', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Draw bold pseudo-frame rectangle
        ctx.strokeStyle = '#222';
        ctx.lineWidth = 3;
        ctx.strokeRect(frame_left, frame_top, frame_w, frame_h);

        // Clip the barrel grid lines to the rectangular frame limits
        ctx.beginPath();
        ctx.rect(frame_left, frame_top, frame_w, frame_h);
        ctx.clip();

        ctx.strokeStyle = '#aaa';
        ctx.lineWidth = 1.5;

        // Vertical grid lines
        for (let gx = -1.8; gx <= 1.81; gx += 0.12) {
            ctx.beginPath();
            for (let gy = -1.0; gy <= 1.01; gy += 0.02) {
                let p = to_pix(distort(gx, gy));
                if (gy < -0.99) ctx.moveTo(p.x, p.y);
                else ctx.lineTo(p.x, p.y);
            }
            ctx.stroke();
        }

        // Horizontal grid lines
        for (let gy = -1.0; gy <= 1.01; gy += 0.12) {
            ctx.beginPath();
            for (let gx = -1.8; gx <= 1.81; gx += 0.02) {
                let p = to_pix(distort(gx, gy));
                if (gx < -1.79) ctx.moveTo(p.x, p.y);
                else ctx.lineTo(p.x, p.y);
            }
            ctx.stroke();
        }

        ctx.restore();
    });

    let marker_r = 0.13;

    // Draw red circle subject to radial distortion and display measured radius
    vid.add_object('marker', { opacity: 0, cx: 0, cy: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Clip marker drawing to the pseudo-frame rectangle
        ctx.save();
        ctx.beginPath();
        ctx.rect(frame_left, frame_top, frame_w, frame_h);
        ctx.clip();

        let num_pts = 100;
        ctx.beginPath();
        for (let i = 0; i <= num_pts; i++) {
            let theta = (i / num_pts) * Math.PI * 2;
            let ux = params.cx + marker_r * Math.cos(theta);
            let uy = params.cy + marker_r * Math.sin(theta);
            let p = to_pix(distort(ux, uy));
            if (i === 0) ctx.moveTo(p.x, p.y);
            else ctx.lineTo(p.x, p.y);
        }
        ctx.fillStyle = 'rgba(238, 44, 44, 0.85)';
        ctx.fill();
        ctx.strokeStyle = '#800';
        ctx.lineWidth = 3;
        ctx.stroke();
        ctx.restore(); // Restore out of clipping region

        // Calculate horizontal diameter and radius under radial distortion
        let left_pt = to_pix(distort(params.cx - marker_r, params.cy));
        let right_pt = to_pix(distort(params.cx + marker_r, params.cy));
        let measured_radius_px = Math.abs(right_pt.x - left_pt.x) / 2;

        // Render "radius = ?" text below the rectangular image frame
        let bottom_y = frame_top + frame_h + 55;
        ctx.font = '32px "Noto Sans Mono"';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(`radius = ${measured_radius_px.toFixed(1)}px`, center_x, bottom_y);

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and barrel distortion grid
    vid.add_transition(['title', 'grid'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. Fade in big red circle near the middle of the frame
    vid.add_transition(['marker'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 0.5;

    // 3. Animate marker moving to the left edge (squished by radial distortion)
    vid.add_transition(['marker'], t, 2.5, { cx: -0.90 });
    t += 2.5 + pause + 0.5;

    // 4. Animate marker moving across to the far right edge to show symmetry
    vid.add_transition(['marker'], t, 3.5, { cx: 0.90 });
    t += 3.5 + pause + 0.5;

    // 5. Return to middle
    vid.add_transition(['marker'], t, 2.0, { cx: 0 });
    t += 2.0 + pause;

    vid.set_duration(t + 1);
    return vid;
}

function part4_marker_size(canvas) {
    let vid = new Timeline();
    vid.set_name("part4_marker_size");


    vid.add_object('title', { opacity: 0, text: 'Minimum Marker Size' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let pix_size = 34;
    let cols = Math.floor((body_grid.width() - 40) / pix_size);
    let rows = Math.floor((body_grid.height() - 20) / pix_size);
    if (cols % 2 === 0) cols--;
    if (rows % 2 === 0) rows--;

    let grid_width = cols * pix_size;
    let grid_height = rows * pix_size;
    let grid_left = body_grid.center().x - grid_width / 2;
    let grid_top = body_grid.center().y - grid_height / 2;

    let mid_c = Math.floor(cols / 2);
    let mid_r = Math.floor(rows / 2);
    let mid_x = grid_left + (mid_c + 0.5) * pix_size;
    let mid_y = grid_top + (mid_r + 0.5) * pix_size;

    let get_spiral_offset = (t) => {
        if (t <= 0 || t >= 1) return { x: 0, y: 0 };
        let r = Math.sin(Math.PI * t) * pix_size * 1.0;
        let theta = t * Math.PI * 2;
        return {
            x: r * Math.cos(theta),
            y: r * Math.sin(theta)
        };
    };

    let get_pixel_coverage = (px, py, size, cx, cy, rad) => {
        let closest_x = Math.max(px, Math.min(cx, px + size));
        let closest_y = Math.max(py, Math.min(cy, py + size));
        let dist_x = cx - closest_x;
        let dist_y = cy - closest_y;
        if (dist_x * dist_x + dist_y * dist_y >= rad * rad) {
            return 0; // Completely outside
        }

        let far_x = Math.max(Math.abs(cx - px), Math.abs(cx - (px + size)));
        let far_y = Math.max(Math.abs(cy - py), Math.abs(cy - (py + size)));
        if (far_x * far_x + far_y * far_y <= rad * rad) {
            return 1; // Completely inside
        }

        // Boundary intersecting pixel: perform 25x25 grid supersampling with smooth distance weighting
        let total_weight = 0;
        let num_steps = 25;
        let step = size / num_steps;
        let half_step = step / 2;
        let diag = half_step * 1.4142;
        for (let ix = 0; ix < num_steps; ix++) {
            let sx = px + half_step + ix * step;
            for (let iy = 0; iy < num_steps; iy++) {
                let sy = py + half_step + iy * step;
                let dist = Math.hypot(sx - cx, sy - cy);
                if (dist <= rad - diag) {
                    total_weight += 1.0;
                } else if (dist < rad + diag) {
                    let f = (rad + diag - dist) / (2 * diag);
                    total_weight += f;
                }
            }
        }
        return total_weight / (num_steps * num_steps);
    };

    vid.add_object('marker_grid', {
        opacity: 0,
        marker_alpha: 0, // Control alpha of the circle marker and its illumination separately
        marker_rad: 0.25, // Starts at 0.5 pixel diameter (0.25 radius)
        spiral1_t: 0,
        spiral2_t: 0
    }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let off1 = get_spiral_offset(params.spiral1_t);
        let off2 = get_spiral_offset(params.spiral2_t);
        let cx = mid_x + off1.x + off2.x;
        let cy = mid_y + off1.y + off2.y;
        let rad = params.marker_rad * pix_size;

        // Boost pixel brightness for sub-pixel marker spot so the small circle is brighter and more visible
        let brightness_boost = Math.max(1.0, 0.8 / (Math.PI * params.marker_rad * params.marker_rad));

        // Render antialiased pixel grid (when marker_alpha is 0, pixels remain pure black)
        for (let r = 0; r < rows; r++) {
            let py = grid_top + r * pix_size;
            for (let c = 0; c < cols; c++) {
                let px = grid_left + c * pix_size;
                let cov = (params.marker_alpha <= 0) ? 0 : get_pixel_coverage(px, py, pix_size, cx, cy, rad);
                let val = Math.round(255 * Math.min(1.0, cov * brightness_boost) * params.marker_alpha);
                ctx.fillStyle = `rgb(${val}, ${val}, ${val})`;
                ctx.fillRect(px, py, pix_size, pix_size);
                ctx.strokeStyle = '#555';
                ctx.lineWidth = 1;
                ctx.strokeRect(px, py, pix_size, pix_size);
            }
        }

        // Bold frame around the pixel grid
        ctx.strokeStyle = '#222';
        ctx.lineWidth = 3;
        ctx.strokeRect(grid_left, grid_top, grid_width, grid_height);

        // Draw red circle indicator over the physical marker boundary
        if (params.marker_alpha > 0) {
            ctx.save();
            ctx.globalAlpha *= params.marker_alpha;
            ctx.beginPath();
            ctx.arc(cx, cy, rad, 0, Math.PI * 2);
            ctx.strokeStyle = '#ff2222';
            ctx.lineWidth = 2.5;
            ctx.stroke();
            ctx.restore();
        }

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and empty pixel grid (black)
    vid.add_transition(['title', 'marker_grid'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. After a pause, start showing the small circle marker in the middle pixel
    vid.add_transition(['marker_grid'], t, 0.5, { marker_alpha: 1 });
    t += 0.5 + pause;

    // 3. Animate one turn spiral movement with 1 pixel radius and back
    vid.add_transition(['marker_grid'], t, 3.5, { spiral1_t: 1 });
    t += 3.5 + pause;

    // 4. Animate circle expanding to 2 pixel radius
    vid.add_transition(['marker_grid'], t, 1.5, { marker_rad: 2.0 });
    t += 1.5 + pause;

    // 5. Repeat the one turn spiral movement with 2 pixel radius marker
    vid.add_transition(['marker_grid'], t, 3.5, { spiral2_t: 1 });
    t += 3.5 + pause;

    // 6. Animate marker becoming very big (full frame height minus 1 pixel)
    let big_rad = (rows - 1) / 2.0; // Diameter becomes rows - 1 pixels
    vid.add_transition(['marker_grid'], t, 2.0, { marker_rad: big_rad });
    t += 2.0 + pause + 0.5;

    vid.set_duration(t);
    return vid;
}

function part4_vignetting(canvas) {
    let vid = new Timeline();

    vid.set_name("part4_vignetting");


    vid.add_object('title', { opacity: 0, text: 'Vignetting' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let pix_size = 34;
    let cols = Math.floor((body_grid.width() - 40) / pix_size);
    let rows = Math.floor((body_grid.height() - 20) / pix_size);
    if (cols % 2 === 0) cols--;
    if (rows % 2 === 0) rows--;

    let grid_width = cols * pix_size;
    let grid_height = rows * pix_size;
    let grid_left = body_grid.center().x - grid_width / 2;
    let grid_top = body_grid.center().y - grid_height / 2;

    let mid_c = Math.floor(cols / 2);
    let mid_r = Math.floor(rows / 2);
    let mid_x = grid_left + (mid_c + 0.5) * pix_size;
    let mid_y = grid_top + (mid_r + 0.5) * pix_size;

    let get_pixel_coverage = (px, py, size, cx, cy, rad) => {
        let closest_x = Math.max(px, Math.min(cx, px + size));
        let closest_y = Math.max(py, Math.min(cy, py + size));
        let dist_x = cx - closest_x;
        let dist_y = cy - closest_y;
        if (dist_x * dist_x + dist_y * dist_y >= rad * rad) {
            return 0; // Completely outside
        }

        let far_x = Math.max(Math.abs(cx - px), Math.abs(cx - (px + size)));
        let far_y = Math.max(Math.abs(cy - py), Math.abs(cy - (py + size)));
        if (far_x * far_x + far_y * far_y <= rad * rad) {
            return 1; // Completely inside
        }

        // Boundary intersecting pixel: perform 25x25 grid supersampling with smooth distance weighting
        let total_weight = 0;
        let num_steps = 25;
        let step = size / num_steps;
        let half_step = step / 2;
        let diag = half_step * 1.4142;
        for (let ix = 0; ix < num_steps; ix++) {
            let sx = px + half_step + ix * step;
            for (let iy = 0; iy < num_steps; iy++) {
                let sy = py + half_step + iy * step;
                let dist = Math.hypot(sx - cx, sy - cy);
                if (dist <= rad - diag) {
                    total_weight += 1.0;
                } else if (dist < rad + diag) {
                    let f = (rad + diag - dist) / (2 * diag);
                    total_weight += f;
                }
            }
        }
        return total_weight / (num_steps * num_steps);
    };

    vid.add_object('vignette_grid', {
        opacity: 0,
        marker_rad: 3.5 // Initial giant marker taking up most of top-left quadrant
    }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Position marker centered in the top-left quadrant with guaranteed clearance inside frame limits
        let max_rad = 3.5;
        let cx = Math.max(grid_left + (max_rad + 1.0) * pix_size, mid_x - 4.2 * pix_size);
        let cy = Math.max(grid_top + (max_rad + 0.8) * pix_size, mid_y - 1.8 * pix_size);
        let rad = params.marker_rad * pix_size;

        // Render antialiased pixel grid with radial vignetting attenuation gain
        for (let r = 0; r < rows; r++) {
            let py = grid_top + r * pix_size;
            let pix_mid_y = py + pix_size / 2;
            for (let c = 0; c < cols; c++) {
                let px = grid_left + c * pix_size;
                let pix_mid_x = px + pix_size / 2;

                let cov = get_pixel_coverage(px, py, pix_size, cx, cy, rad);
                let dist_to_center = Math.hypot(pix_mid_x - mid_x, pix_mid_y - mid_y) / pix_size;

                // Vignetting gain drops quadratically with radial distance from optical frame center
                let gain = Math.max(0.25, 1.0 - 0.010 * (dist_to_center * dist_to_center));

                let val = Math.round(255 * Math.min(1.0, cov * gain));
                ctx.fillStyle = `rgb(${val}, ${val}, ${val})`;
                ctx.fillRect(px, py, pix_size, pix_size);
                ctx.strokeStyle = '#555';
                ctx.lineWidth = 1;
                ctx.strokeRect(px, py, pix_size, pix_size);
            }
        }

        // Bold frame around the pixel grid
        ctx.strokeStyle = '#222';
        ctx.lineWidth = 3;
        ctx.strokeRect(grid_left, grid_top, grid_width, grid_height);

        // Draw red circle indicator over the physical marker boundary
        ctx.beginPath();
        ctx.arc(cx, cy, rad, 0, Math.PI * 2);
        ctx.strokeStyle = '#ff2222';
        ctx.lineWidth = 2.5;
        ctx.stroke();

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and initial giant marker showing asymmetric vignetting dimming
    vid.add_transition(['title', 'vignette_grid'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 1.5;

    // 2. Animate circle decreasing in size to a normal marker radius (~1.2 pixels)
    vid.add_transition(['vignette_grid'], t, 2.5, { marker_rad: 1.2 });
    t += 2.5 + pause + 1.5;

    vid.set_duration(t);
    return vid;
}

function part5_mounting(canvas) {
    let vid = new Timeline();

    vid.set_name("part5_mounting");

    vid.add_object('title', { opacity: 0, text: 'Camera Mounting' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let room_left_x = body_grid.left_center().x;
    let room_right_x = body_grid.right_center().x;
    let ceiling_y = body_grid.top_center().y - 10;
    let floor_y = body_grid.bottom_center().y - 30;

    let cam_tl = { x: room_left_x + 18, y: ceiling_y + 40 };
    let cam_mid = { x: room_left_x + 18, y: floor_y - 85 };
    let cam_tl2 = { x: room_left_x + 18, y: ceiling_y + 120 };

    vid.add_object('mounting_scene', {
        opacity: 0,
        floor_opacity: 0,
        figure_opacity: 0,
        cam_tl_opacity: 0,
        rays_opacity: 0,
        slide_progress: 0, // 0 = centered, 1 = slid left causing ray occlusion with left arm
        cam_mid_opacity: 0,
        mid_rays_opacity: 0,
        cone1_opacity: 0,
        fov_deg: 49,
        cam1_tilt: 45, // downward optical axis angle in deg from horizontal right
        angle_marker_opacity: 0,
        blind_spot_bl_opacity: 0,
        blind_spot_tr_opacity: 0,
        cam_tl2_opacity: 0,
        cone2_opacity: 0
    }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Stick figure slides smoothly from center toward left wall
        let fig_x = (room_left_x + room_right_x) * 0.5 - params.slide_progress * 220;

        // 1. Draw floor box
        if (params.floor_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.floor_opacity;
            let floor_box_h = 46;
            ctx.fillStyle = '#111';
            ctx.fillRect(room_left_x, floor_y, room_right_x - room_left_x, floor_box_h);
            ctx.strokeStyle = '#333';
            ctx.lineWidth = 2;
            ctx.strokeRect(room_left_x, floor_y, room_right_x - room_left_x, floor_box_h);
            ctx.fillStyle = '#fff';
            ctx.font = 'bold 24px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('Floor', (room_left_x + room_right_x) / 2, floor_y + floor_box_h / 2);
            ctx.restore();
        }

        // Helper to draw FOV cone with exact room clipping
        let draw_fov_cone = (cam_pos, tilt_deg, fov_deg, op) => {
            if (op <= 0) return;
            ctx.save();
            ctx.globalAlpha *= op;
            let th1 = (tilt_deg - fov_deg / 2) * (Math.PI / 180);
            let th2 = (tilt_deg + fov_deg / 2) * (Math.PI / 180);

            ctx.save();
            ctx.beginPath();
            ctx.rect(room_left_x - 50, ceiling_y - 100, (room_right_x - room_left_x) + 100, (floor_y - ceiling_y) + 100);
            ctx.clip();

            ctx.beginPath();
            ctx.moveTo(cam_pos.x, cam_pos.y);
            ctx.arc(cam_pos.x, cam_pos.y, 1600, th1, th2, false);
            ctx.closePath();
            ctx.fillStyle = 'rgba(255, 215, 0, 0.28)';
            ctx.fill();
            ctx.restore();

            // Draw crisp yellow boundary lines clipped cleanly at room walls
            ctx.strokeStyle = 'rgba(230, 160, 0, 0.85)';
            ctx.lineWidth = 2.5;
            ctx.save();
            ctx.beginPath();
            ctx.rect(room_left_x - 50, ceiling_y - 100, (room_right_x - room_left_x) + 100, (floor_y - ceiling_y) + 100);
            ctx.clip();
            ctx.beginPath();
            ctx.moveTo(cam_pos.x, cam_pos.y);
            ctx.lineTo(cam_pos.x + 1600 * Math.cos(th1), cam_pos.y + 1600 * Math.sin(th1));
            ctx.moveTo(cam_pos.x, cam_pos.y);
            ctx.lineTo(cam_pos.x + 1600 * Math.cos(th2), cam_pos.y + 1600 * Math.sin(th2));
            ctx.stroke();
            ctx.restore();
            ctx.restore();
        };

        // Draw light cones (behind cameras and figure)
        draw_fov_cone(cam_tl, params.cam1_tilt, params.fov_deg, params.cone1_opacity);
        draw_fov_cone(cam_tl2, 21, 42, params.cone2_opacity);

        // 2. Stick figure and markers
        let arm_shoulder = { x: fig_x - 2, y: floor_y - 200 };
        let arm_hand = { x: fig_x - 55, y: floor_y - 130 };

        let dots = [
            { x: fig_x - 22, y: floor_y - 238 }, // Head front
            { x: fig_x - 2, y: floor_y - 180 },  // Chest
            { x: fig_x - 4, y: floor_y - 95 },   // Hip / thigh (lowered slightly so centered ray clears arm)
            { x: fig_x - 12, y: floor_y - 50 },  // Knee / shin
            { x: fig_x - 25, y: floor_y - 10 },  // Left foot
            { x: arm_hand.x, y: arm_hand.y },    // Left hand tip
            { x: fig_x + 32, y: floor_y - 150 }, // Right elbow/hand
            { x: fig_x + 22, y: floor_y - 50 }   // Right leg
        ];

        let dot_hip = dots[2];
        let dot_knee = dots[3];

        if (params.figure_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.figure_opacity;
            ctx.strokeStyle = '#222';
            ctx.lineWidth = 5;
            ctx.lineCap = 'round';
            ctx.lineJoin = 'round';

            // Head
            ctx.beginPath();
            ctx.arc(fig_x, floor_y - 240, 22, 0, Math.PI * 2);
            ctx.stroke();

            // Spine & Torso
            ctx.beginPath();
            ctx.moveTo(fig_x, floor_y - 218);
            ctx.lineTo(fig_x, floor_y - 110);
            ctx.stroke();

            // Legs
            ctx.beginPath();
            ctx.moveTo(fig_x, floor_y - 110);
            ctx.lineTo(fig_x - 25, floor_y - 10);
            ctx.moveTo(fig_x, floor_y - 110);
            ctx.lineTo(fig_x + 25, floor_y - 10);
            ctx.stroke();

            // Arms
            ctx.beginPath();
            ctx.moveTo(fig_x, floor_y - 195);
            ctx.lineTo(fig_x + 35, floor_y - 140);     // Right arm
            ctx.moveTo(arm_shoulder.x, arm_shoulder.y);
            ctx.lineTo(arm_hand.x, arm_hand.y);        // Left arm
            ctx.stroke();

            // Markers sticking uniformly around outside
            for (let d of dots) {
                ctx.beginPath();
                ctx.arc(d.x, d.y, 6.5, 0, Math.PI * 2);
                ctx.fillStyle = '#ccc';
                ctx.fill();
                ctx.strokeStyle = '#000';
                ctx.lineWidth = 2;
                ctx.stroke();
            }
            ctx.restore();
        }

        // Helper to draw compact square camera
        let draw_camera_unit = (pos, tilt_deg, op) => {
            if (op <= 0) return;
            ctx.save();
            ctx.globalAlpha *= op;
            ctx.translate(pos.x, pos.y);
            ctx.rotate((tilt_deg * Math.PI) / 180);

            let size = 36;
            // Nozzle / Frustum
            ctx.beginPath();
            ctx.moveTo(size / 2, -9);
            ctx.lineTo(size / 2 + 18, -16);
            ctx.lineTo(size / 2 + 18, 16);
            ctx.lineTo(size / 2, 9);
            ctx.closePath();
            ctx.fillStyle = '#ddd';
            ctx.fill();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2.5;
            ctx.stroke();

            // Square main body
            ctx.fillStyle = '#222';
            ctx.fillRect(-size / 2, -size / 2, size, size);
            ctx.strokeRect(-size / 2, -size / 2, size, size);
            ctx.restore();
        };

        // Helper for exact line segment intersection (ray vs left arm)
        let get_intersection = (p1, p2, q1, q2) => {
            let s1_x = p2.x - p1.x, s1_y = p2.y - p1.y;
            let s2_x = q2.x - q1.x, s2_y = q2.y - q1.y;
            let denom = s1_x * s2_y - s2_x * s1_y;
            if (Math.abs(denom) < 0.0001) return null;

            let t = (-s1_y * (p1.x - q1.x) + s1_x * (p1.y - q1.y)) / denom;
            let u = (s2_x * (p1.y - q1.y) - s2_y * (p1.x - q1.x)) / denom;

            // Include small tolerance for arm bone line width
            if (t >= 0 && t <= 1 && u >= -0.05 && u <= 1.05) {
                return { x: p1.x + t * s1_x, y: p1.y + t * s1_y };
            }
            return null;
        };

        // 3. Draw dashed rays emanating from exact center of ceiling camera box with live occlusion tracking loss
        if (params.rays_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.rays_opacity;
            ctx.setLineDash([8, 6]);
            ctx.lineWidth = 2.5;

            let start_x = cam_tl.x;
            let start_y = cam_tl.y;

            let draw_ray_to = (target) => {
                let hit = get_intersection({ x: start_x, y: start_y }, target, arm_shoulder, arm_hand);
                if (hit && params.slide_progress > 0.15) {
                    // Occluded by left arm! Ray stops instantly at arm intersection with much lower opacity gray
                    ctx.save();
                    ctx.globalAlpha *= 0.22;
                    ctx.strokeStyle = '#999';
                    ctx.beginPath();
                    ctx.moveTo(start_x, start_y);
                    ctx.lineTo(hit.x, hit.y);
                    ctx.stroke();
                    ctx.restore();
                } else {
                    // Unobstructed line of sight! Ray reaches marker in bright red
                    ctx.strokeStyle = '#ff2222';
                    ctx.beginPath();
                    ctx.moveTo(start_x, start_y);
                    ctx.lineTo(target.x, target.y);
                    ctx.stroke();
                }
            };

            draw_ray_to(dot_hip);
            draw_ray_to(dot_knee);
            ctx.restore();
        }

        // 4. Draw rays from exact center of mid-wall camera box recovering occluded markers
        if (params.mid_rays_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.mid_rays_opacity;
            ctx.setLineDash([8, 6]);
            ctx.strokeStyle = '#ff2222';
            ctx.lineWidth = 2.5;
            let sx = cam_mid.x;
            let sy = cam_mid.y;

            ctx.beginPath();
            ctx.moveTo(sx, sy);
            ctx.lineTo(dot_hip.x, dot_hip.y);
            ctx.moveTo(sx, sy);
            ctx.lineTo(dot_knee.x, dot_knee.y);
            ctx.stroke();
            ctx.restore();
        }

        // Render solid camera units on top so rays emerge cleanly from the center axis
        draw_camera_unit(cam_tl, params.cam1_tilt, params.cam_tl_opacity);
        draw_camera_unit(cam_mid, 5, params.cam_mid_opacity);
        draw_camera_unit(cam_tl2, 21, params.cam_tl2_opacity);

        // 5. CAD style angle marker ("42°" -> "49°")
        if (params.angle_marker_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.angle_marker_opacity;
            let r_ang = 115;
            let th1 = (params.cam1_tilt - params.fov_deg / 2) * (Math.PI / 180);
            let th2 = (params.cam1_tilt + params.fov_deg / 2) * (Math.PI / 180);

            ctx.beginPath();
            ctx.arc(cam_tl.x, cam_tl.y, r_ang, th1, th2);
            ctx.strokeStyle = '#111';
            ctx.lineWidth = 2;
            ctx.stroke();

            // Arrowheads on CAD arc
            let draw_arc_arrow = (ang, forward) => {
                let ax = cam_tl.x + r_ang * Math.cos(ang);
                let ay = cam_tl.y + r_ang * Math.sin(ang);
                let dir = ang + (forward ? 1 : -1) * (Math.PI / 2);
                ctx.beginPath();
                ctx.moveTo(ax, ay);
                ctx.lineTo(ax - 10 * Math.cos(dir - 0.45), ay - 10 * Math.sin(dir - 0.45));
                ctx.lineTo(ax - 10 * Math.cos(dir + 0.45), ay - 10 * Math.sin(dir + 0.45));
                ctx.closePath();
                ctx.fillStyle = '#111';
                ctx.fill();
            };
            draw_arc_arrow(th1, true);
            draw_arc_arrow(th2, false);

            // Lead pointer and degree text
            let mid_th = (th1 + th2) / 2;
            let tx = cam_tl.x + 185 * Math.cos(mid_th);
            let ty = cam_tl.y + 185 * Math.sin(mid_th);

            ctx.beginPath();
            ctx.moveTo(cam_tl.x + (r_ang + 4) * Math.cos(mid_th), cam_tl.y + (r_ang + 4) * Math.sin(mid_th));
            ctx.lineTo(tx - 10, ty);
            ctx.strokeStyle = '#444';
            ctx.lineWidth = 1.5;
            ctx.stroke();

            ctx.fillStyle = '#000';
            ctx.font = 'bold 24px "Noto Sans Mono", monospace';
            ctx.textAlign = 'left';
            ctx.textBaseline = 'middle';
            ctx.fillText(`${Math.round(params.fov_deg)}°`, tx, ty);
            ctx.restore();
        }

        // 6. Blind Spot text indicators
        if (params.blind_spot_bl_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.blind_spot_bl_opacity;
            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 28px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            let bs_x = room_left_x + 95;
            let bs_y = floor_y - 75;
            ctx.fillText('Blind', bs_x, bs_y - 18);
            ctx.fillText('Spot', bs_x, bs_y + 18);
            ctx.restore();
        }

        if (params.blind_spot_tr_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.blind_spot_tr_opacity;
            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 28px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            let bs_x = room_left_x + (room_right_x - room_left_x) * 0.45;
            let bs_y = ceiling_y + 75;
            ctx.fillText('Blind', bs_x, bs_y - 18);
            ctx.fillText('Spot', bs_x, bs_y + 18);
            ctx.restore();
        }

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and floor box near bottom of slide
    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    vid.add_transition(['mounting_scene'], t, 0.5, { opacity: 1, floor_opacity: 1 });
    t += 0.5 + pause;

    // 2. Animate in stick figure person centered in frame
    vid.add_transition(['mounting_scene'], t, 0.6, { figure_opacity: 1 });
    t += 0.6 + pause;

    // 3. Fade in square camera box near top-left angled partially down
    vid.add_transition(['mounting_scene'], t, 0.5, { cam_tl_opacity: 1 });
    t += 0.5 + pause;

    // 4. Animate two red dashed lines going to dots lower down on the body (clearing arm)
    vid.add_transition(['mounting_scene'], t, 0.5, { rays_opacity: 1 });
    t += 0.5 + pause + 0.5;

    // 5. Slide stick figure to the left; rays intersect arm, stop at collision, and become gray (tracking loss)
    vid.add_transition(['mounting_scene'], t, 2.0, { slide_progress: 1 });
    t += 2.0 + pause + 0.5;

    // 6. Fade in camera midway up left wall pointing right with rays recovering occluded dots
    vid.add_transition(['mounting_scene'], t, 0.5, { cam_mid_opacity: 1, mid_rays_opacity: 1 });
    t += 0.5 + pause + 1.5;

    // 7. Fade out mid camera & rays, and slide stick figure back to center
    vid.add_transition(['mounting_scene'], t, 1.5, { cam_mid_opacity: 0, mid_rays_opacity: 0, rays_opacity: 0, slide_progress: 0 });
    t += 1.5 + pause;

    // 8. Fade in yellow cone of light from top-left camera covering visible FOV
    vid.add_transition(['mounting_scene'], t, 0.5, { cone1_opacity: 1 });
    t += 0.5 + pause;

    // 9. Fade in CAD style angle marker on cone saying "49°"
    vid.add_transition(['mounting_scene'], t, 0.5, { angle_marker_opacity: 1 });
    t += 0.5 + pause + 0.5;

    // 10. Animate FOV changing to 42 degrees (cone narrows to compensate)
    vid.add_transition(['mounting_scene'], t, 1.5, { fov_deg: 42 });
    t += 1.5 + pause + 0.5;

    // 11. Animate camera tilting up so top of field of view is roughly horizontal (tilt = 21)
    vid.add_transition(['mounting_scene'], t, 1.5, { cam1_tilt: 21, angle_marker_opacity: 0 });
    t += 1.5 + pause;

    // 12. Fade in red text at bottom-left room blind spot outside cone reading "Blind\nSpot"
    vid.add_transition(['mounting_scene'], t, 0.5, { blind_spot_bl_opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 13. Tilt camera down so one edge of FOV is vertical (tilt = 69), fade out bottom-left blind spot, fade in top-right blind spot
    vid.add_transition(['mounting_scene'], t, 1.5, { blind_spot_bl_opacity: 0, cam1_tilt: 69 });
    t += 1.5;
    vid.add_transition(['mounting_scene'], t, 0.5, { blind_spot_tr_opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 14. Fade in another camera mounted below pointing higher up with its cone to show complete coverage
    vid.add_transition(['mounting_scene'], t, 1.0, { blind_spot_tr_opacity: 0, cam_tl2_opacity: 1, cone2_opacity: 1 });
    t += 1.0 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

function part6_wiring(canvas) {
    let vid = new Timeline();

    vid.set_name("part6_wiring");

    vid.add_object('title', { opacity: 0, text: 'Network Wiring' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let top_y = body_grid.top_center().y;
    let bottom_y = body_grid.bottom_center().y - 35; // Leave extra vertical space at bottom for text labels
    let full_height = bottom_y - top_y;
    let mid_y = top_y + full_height / 2;

    let num_cams = 8;
    let cam_w = 140;
    let right_w = 175;

    let left_x = body_grid.left_center().x + cam_w / 2;
    let mid_x = body_grid.center().x;
    let right_x = body_grid.right_center().x - right_w / 2;

    let switch_box = new DiagramBox({
        text: 'Network\nSwitch',
        width: 125,
        height: full_height,
        font_size: 24,
        position: { x: mid_x, y: mid_y }
    });

    let cam_h = 36;
    let cam_gap = (full_height - num_cams * cam_h) / (num_cams - 1);

    let cam_boxes = [];
    for (let i = 0; i < num_cams; i++) {
        let cy = top_y + cam_h / 2 + i * (cam_h + cam_gap);
        cam_boxes.push(new DiagramBox({
            text: 'Camera',
            width: cam_w,
            height: cam_h,
            font_size: 18,
            position: { x: left_x, y: cy }
        }));
    }

    let right_h = 85;
    let home_net_y = top_y + full_height * 0.25;
    let computer_y = top_y + full_height * 0.75;

    let home_net_box = new DiagramBox({
        text: 'Home\nNetwork',
        width: right_w,
        height: right_h,
        font_size: 22,
        position: { x: right_x, y: home_net_y }
    });

    let computer_box = new DiagramBox({
        text: 'Computer',
        width: right_w,
        height: right_h,
        font_size: 22,
        position: { x: right_x, y: computer_y }
    });

    vid.add_object('wiring_diagram', {
        opacity: 0,
        dedicated_box_opacity: 0,
        packet_progress: -1,
        switch_to_home_opacity: 1,
        switch_to_comp_opacity: 0,
        direct_box_opacity: 0
    }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // 1. Draw connecting arrows first (under boxes)
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';

        // Left column bi-directional arrows between cameras and switch
        for (let i = 0; i < num_cams; i++) {
            let cy = cam_boxes[i].position().y;
            let from_p = { x: left_x + cam_w / 2 + 6, y: cy };
            let to_p = { x: mid_x - 125 / 2 - 6, y: cy };
            drawArrowPos(ctx, from_p, to_p, 2.5, 9, true);
        }

        // Right column bi-directional arrow between switch and home network
        if (params.switch_to_home_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.switch_to_home_opacity;
            let from_h = { x: mid_x + 125 / 2 + 6, y: home_net_y };
            let to_h = { x: right_x - right_w / 2 - 6, y: home_net_y };
            drawArrowPos(ctx, from_h, to_h, 3, 11, true);
            ctx.restore();
        }

        // Right column connection between Home Network and Computer
        let from_hc = { x: right_x, y: home_net_y + right_h / 2 + 6 };
        let to_hc = { x: right_x, y: computer_y - right_h / 2 - 6 };
        drawArrowPos(ctx, from_hc, to_hc, 3, 11, true);

        // Right column direct bi-directional arrow between switch and computer
        if (params.switch_to_comp_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.switch_to_comp_opacity;
            let from_c = { x: mid_x + 125 / 2 + 6, y: computer_y };
            let to_c = { x: right_x - right_w / 2 - 6, y: computer_y };
            drawArrowPos(ctx, from_c, to_c, 3, 11, true);
            ctx.restore();
        }

        // 2. Draw Diagram Boxes
        switch_box.draw(ctx);
        for (let b of cam_boxes) b.draw(ctx);
        home_net_box.draw(ctx);
        computer_box.draw(ctx);

        // 3. Dedicated Switch(s) red outline box and label
        if (params.dedicated_box_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.dedicated_box_opacity;
            ctx.strokeStyle = '#dd1111';
            ctx.lineWidth = 3.5;
            ctx.setLineDash([10, 7]);
            let bx1 = (left_x - cam_w / 2) - 12;
            let bx2 = (mid_x + 125 / 2) + 12;
            let by1 = top_y - 12;
            let by2 = bottom_y + 12;
            ctx.strokeRect(bx1, by1, bx2 - bx1, by2 - by1);
            ctx.setLineDash([]);

            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 26px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillText('Dedicated Switch(s)', (bx1 + bx2) / 2, by2 + 10);
            ctx.restore();
        }

        // 4. Animate 5 packets passing from camera #1 along wire to switch and broadcasting to other cameras
        if (params.packet_progress >= 0 && params.packet_progress < 5.0 && params.dedicated_box_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.dedicated_box_opacity;
            let t_cycle = params.packet_progress % 1;

            let draw_packet = (x, y) => {
                ctx.beginPath();
                ctx.arc(x, y, 7, 0, Math.PI * 2);
                ctx.fillStyle = '#ff2222';
                ctx.fill();
                ctx.strokeStyle = '#fff';
                ctx.lineWidth = 1.5;
                ctx.stroke();
            };

            if (t_cycle < 0.42) {
                // Packet traveling from Camera #1 along the wire to the switch
                let frac = t_cycle / 0.42;
                let sx = left_x + cam_w / 2;
                let sy = cam_boxes[0].position().y;
                let tx = mid_x - 125 / 2;
                let px = sx + (tx - sx) * frac;
                draw_packet(px, sy);
            } else if (t_cycle <= 0.92) {
                // Switch broadcasts packet out along wires to Cameras #2 through #8
                let frac = (t_cycle - 0.42) / 0.50;
                let sx = mid_x - 125 / 2;
                for (let j = 1; j < num_cams; j++) {
                    let sy = cam_boxes[j].position().y;
                    let tx = left_x + cam_w / 2;
                    let px = sx + (tx - sx) * frac;
                    draw_packet(px, sy);
                }
            }
            ctx.restore();
        }

        // 5. Direct Connection red outline box around arrow and label
        if (params.direct_box_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.direct_box_opacity;
            ctx.strokeStyle = '#dd1111';
            ctx.lineWidth = 3.5;
            ctx.setLineDash([10, 7]);
            let dx1 = (mid_x + 125 / 2) - 4;
            let dx2 = (right_x - right_w / 2) + 4;
            let dy1 = computer_y - 28;
            let dy2 = computer_y + 28;
            ctx.strokeRect(dx1, dy1, dx2 - dx1, dy2 - dy1);
            ctx.setLineDash([]);

            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 24px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillText('Direct Connection', (dx1 + dx2) / 2, dy2 + 10);
            ctx.restore();
        }

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and the 3-column wiring diagram blocks
    vid.add_transition(['title', 'wiring_diagram'], t, 0.8, { opacity: 1 });
    t += 0.8 + pause;

    // 2. Fade in red outline box around switch & cameras reading "Dedicated Switch(s)"
    vid.add_transition(['wiring_diagram'], t, 0.5, { dedicated_box_opacity: 1 });
    t += 0.5;

    // 3. Immediately after fade in, animate 5 packets passing from camera #1 to switch and broadcasting to others
    vid.add_transition(['wiring_diagram'], t, 4.5, { packet_progress: 5.0 });
    t += 4.5 + pause;

    // 4. Fade out red outline and text
    vid.add_transition(['wiring_diagram'], t, 0.6, { dedicated_box_opacity: 0 });
    t += 0.6 + pause;

    // 5. Fade out arrow to home network, fade in arrow between computer and switch, and fade in Direct Connection outline box
    vid.add_transition(['wiring_diagram'], t, 0.8, { switch_to_home_opacity: 0, switch_to_comp_opacity: 1, direct_box_opacity: 1 });
    t += 0.8 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part7_recap(canvas) {
    let vid = new Timeline();
    vid.set_name("part7_recap");

    vid.add_object('title', { opacity: 0, text: 'Recap' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let center_y = body_grid.center().y;
    let cam_x = body_grid.left_center().x + 70;

    let cam1_pos = { x: cam_x, y: center_y - 130 };
    let cam2_pos = { x: cam_x, y: center_y + 130 };
    let marker_pos = { x: body_grid.right_center().x - 220, y: center_y };

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

    let target_tilt1 = Math.atan2(marker_pos.y - cam1_pos.y, marker_pos.x - cam1_pos.x) * (180 / Math.PI);
    let target_tilt2 = Math.atan2(marker_pos.y - cam2_pos.y, marker_pos.x - cam2_pos.x) * (180 / Math.PI);

    vid.add_object('recap_scene', {
        opacity: 0,
        cam_opacity: 1,
        cam1_tilt: 0,
        cam2_tilt: 0,
        rays_opacity: 0,
        purple_line_opacity: 0
    }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let ray1_start = { x: cam1_pos.x + (cam1_box._width / 2), y: cam1_pos.y };
        let ray2_start = { x: cam2_pos.x + (cam2_box._width / 2), y: cam2_pos.y };

        // 1. Draw observation rays and initial connecting triangle baseline
        if (params.rays_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.rays_opacity;
            ctx.lineWidth = 3;

            // Base line between cameras completing the triangle
            ctx.beginPath();
            ctx.moveTo(ray1_start.x, ray1_start.y);
            ctx.lineTo(ray2_start.x, ray2_start.y);
            ctx.strokeStyle = '#ff9999';
            ctx.stroke();

            // Rays to marker
            ctx.setLineDash([10, 10]);
            ctx.strokeStyle = '#f00';

            ctx.beginPath();
            ctx.moveTo(ray1_start.x, ray1_start.y);
            ctx.lineTo(marker_pos.x, marker_pos.y);
            ctx.stroke();

            ctx.beginPath();
            ctx.moveTo(ray2_start.x, ray2_start.y);
            ctx.lineTo(marker_pos.x, marker_pos.y);
            ctx.stroke();
            ctx.restore();
        }

        // 2. Draw purple bolded line between cameras with pointing arrow and label
        if (params.purple_line_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.purple_line_opacity;

            // Bold purple line along the triangle baseline
            ctx.strokeStyle = '#a200ff';
            ctx.lineWidth = 6;
            ctx.beginPath();
            ctx.moveTo(ray1_start.x, ray1_start.y);
            ctx.lineTo(ray2_start.x, ray2_start.y);
            ctx.stroke();

            // Text label and pointing arrow
            let label_x = ray1_start.x + 220;
            let label_y = center_y;
            ctx.font = 'bold 26px sans-serif';
            ctx.fillStyle = '#a200ff';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('Need to find this', label_x, label_y);

            let from_p = { x: label_x - 120, y: label_y };
            let to_p = { x: ray1_start.x + 8, y: label_y };
            ctx.strokeStyle = '#a200ff';
            drawArrowPos(ctx, from_p, to_p, 3.5, 12, false);

            ctx.restore();
        }

        // 3. Draw Cameras anchored at front edge (ray start) with text fading out when box fades down
        let draw_cam = (box, ray_p, tilt_deg) => {
            ctx.save();
            ctx.globalAlpha *= params.cam_opacity;
            ctx.translate(ray_p.x, ray_p.y);
            ctx.rotate((tilt_deg * Math.PI) / 180);

            // Frustum / Nozzle directly in front of ray origin
            ctx.fillStyle = '#eee';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.moveTo(0, -20);
            ctx.lineTo(40, -40);
            ctx.lineTo(40, 40);
            ctx.lineTo(0, 20);
            ctx.closePath();
            ctx.fill();
            ctx.stroke();

            // Main Camera Box behind ray origin
            let old_pos = box._position;
            let old_color = box._text_color;
            box._position = { x: -(box._width / 2), y: 0 };
            let text_alpha = Math.max(0, Math.min(1, (params.cam_opacity - 0.5) * 2));
            box._text_color = `rgba(0, 0, 0, ${text_alpha})`;
            box.draw(ctx);
            box._position = old_pos;
            box._text_color = old_color || '#000000';
            ctx.restore();
        };

        draw_cam(cam1_box, ray1_start, params.cam1_tilt);
        draw_cam(cam2_box, ray2_start, params.cam2_tilt);

        // 4. Draw Marker
        ctx.save();
        ctx.translate(marker_pos.x, marker_pos.y);
        ctx.beginPath();
        ctx.arc(0, 0, 16, 0, 2 * Math.PI);
        ctx.fillStyle = '#ddd';
        ctx.fill();
        ctx.lineWidth = 2.5;
        ctx.strokeStyle = '#000';
        ctx.stroke();

        ctx.font = 'bold 22px sans-serif';
        ctx.fillStyle = '#000';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText('Marker', 0, -26);
        ctx.restore();

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title
    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. Fade in cameras and marker
    vid.add_transition(['recap_scene'], t, 0.8, { opacity: 1 });
    t += 0.8 + pause;

    // 3. Slightly tilt the cameras towards the marker
    vid.add_transition(['recap_scene'], t, 1.2, { cam1_tilt: target_tilt1, cam2_tilt: target_tilt2 });
    t += 1.2 + pause;

    // 4. Draw in all the rays going from the cameras to the marker
    vid.add_transition(['recap_scene'], t, 0.8, { rays_opacity: 1 });
    t += 0.8 + pause + 0.5;

    // 5. Draw in the purple bold line between cameras with arrow and text reading "Need to find this"
    vid.add_transition(['recap_scene'], t, 0.8, { purple_line_opacity: 1, cam_opacity: 0.5 });
    t += 0.8 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export async function part7_relative(canvas) {
    let vid = new Timeline();
    vid.set_name("part7_relative");

    console.log('AAAA');

    let eq_T1 = await math_to_img(String.raw`\mathbf{T}_1`);

    console.log('BBB');

    let eq_T2 = await math_to_img(String.raw`\mathbf{T}_2`);
    let eq_rel = await math_to_img(String.raw`\mathbf{T}_2 \mathbf{T}_1^{-1}`);

    vid.add_object('title', { opacity: 0, text: 'Relative Pose' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let body_grid = slide_body_grid(canvas);
    let center_y = body_grid.center().y;
    let cam_x = body_grid.left_center().x + 70;

    let cam1_pos = { x: cam_x, y: center_y - 130 };
    let cam2_pos = { x: cam_x, y: center_y + 130 };
    let wand_pos = { x: body_grid.right_center().x - 240, y: center_y };

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

    let ray1_start = { x: cam1_pos.x + (cam1_box._width / 2), y: cam1_pos.y };
    let ray2_start = { x: cam2_pos.x + (cam2_box._width / 2), y: cam2_pos.y };

    let tilt1 = Math.atan2(wand_pos.y - ray1_start.y, wand_pos.x - ray1_start.x) * (180 / Math.PI);
    let tilt2 = Math.atan2(wand_pos.y - ray2_start.y, wand_pos.x - ray2_start.x) * (180 / Math.PI);

    vid.add_object('relative_scene', {
        opacity: 0,
        rays_opacity: 0,
        t_labels_opacity: 0,
        pnp_text_opacity: 0,
        purple_line_opacity: 0
    }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // 1. Draw observation rays and initial baseline triangle
        if (params.rays_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.rays_opacity;
            ctx.lineWidth = 3;

            ctx.beginPath();
            ctx.moveTo(ray1_start.x, ray1_start.y);
            ctx.lineTo(ray2_start.x, ray2_start.y);
            ctx.strokeStyle = '#ff9999';
            ctx.stroke();

            ctx.setLineDash([10, 10]);
            ctx.strokeStyle = '#f00';

            ctx.beginPath();
            ctx.moveTo(ray1_start.x, ray1_start.y);
            ctx.lineTo(wand_pos.x, wand_pos.y);
            ctx.stroke();

            ctx.beginPath();
            ctx.moveTo(ray2_start.x, ray2_start.y);
            ctx.lineTo(wand_pos.x, wand_pos.y);
            ctx.stroke();
            ctx.restore();
        }

        // 2. Draw purple baseline, attached T2 T1^-1 equation, and multiline pointer text
        if (params.purple_line_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.purple_line_opacity;

            ctx.strokeStyle = '#a200ff';
            ctx.lineWidth = 6;
            ctx.beginPath();
            ctx.moveTo(ray1_start.x, ray1_start.y);
            ctx.lineTo(ray2_start.x, ray2_start.y);
            ctx.stroke();

            // Attached LaTeX label T2 T1^-1 next to purple line
            let rel_eq_x = ray1_start.x + 65;
            ctx.save();
            ctx.translate(rel_eq_x, center_y);
            ctx.scale(1.7 * (1 / math_scale()), 1.7 * (1 / math_scale()));
            ctx.drawImage(eq_rel, -eq_rel.width / 2, -eq_rel.height / 2);
            ctx.restore();

            // Two-line text and purple pointing arrow
            let label_x = ray1_start.x + 320;
            let label_y = center_y;
            ctx.font = 'bold 24px sans-serif';
            ctx.fillStyle = '#a200ff';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('Find relative', label_x, label_y - 15);
            ctx.fillText('transform matrix', label_x, label_y + 15);

            let from_p = { x: label_x - 115, y: label_y };
            let eq_half_w = (eq_rel.width * (1.7 * (1 / math_scale()))) / 2;
            let to_p = { x: rel_eq_x + eq_half_w + 10, y: label_y };
            ctx.strokeStyle = '#a200ff';
            drawArrowPos(ctx, from_p, to_p, 3.5, 12, false);

            ctx.restore();
        }

        // 3. Draw Cameras at 0.5 decreased opacity, already tilted and aligned at triangle vertices without text
        let draw_cam = (box, ray_p, tilt_deg) => {
            ctx.save();
            ctx.globalAlpha *= 0.5;
            ctx.translate(ray_p.x, ray_p.y);
            ctx.rotate((tilt_deg * Math.PI) / 180);

            ctx.fillStyle = '#eee';
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.moveTo(0, -20);
            ctx.lineTo(40, -40);
            ctx.lineTo(40, 40);
            ctx.lineTo(0, 20);
            ctx.closePath();
            ctx.fill();
            ctx.stroke();

            let old_pos = box._position;
            let old_color = box._text_color;
            box._position = { x: -(box._width / 2), y: 0 };
            box._text_color = 'rgba(0, 0, 0, 0)';
            box.draw(ctx);
            box._position = old_pos;
            box._text_color = old_color || '#000000';
            ctx.restore();
        };

        draw_cam(cam1_box, ray1_start, tilt1);
        draw_cam(cam2_box, ray2_start, tilt2);

        // 4. Draw T-Wand target with segmented bars (no black underneath markers or protruding past ends) and long handle
        ctx.save();
        ctx.translate(wand_pos.x, wand_pos.y);
        ctx.rotate((-12 * Math.PI) / 180);

        let scale_m = 380; // Scale: pixels per meter
        let r = 13; // Segment cutoff offset to prevent overlapping black under the 14px markers

        ctx.fillStyle = '#111';
        ctx.strokeStyle = '#000';

        let y1 = -0.25 * scale_m;
        let y2 = 0;
        let y3 = 0.125 * scale_m;
        let x4 = 0.2 * scale_m;
        let x_end = 0.45 * scale_m; // Extended long handle past (0.2, 0)

        // Vertical T-bar segments (strictly bounded between the circles, zero protruding tips):
        ctx.fillRect(-6, y1 + r, 12, (y2 - r) - (y1 + r));
        ctx.fillRect(-6, y2 + r, 12, (y3 - r) - (y2 + r));

        // Horizontal Handle segments:
        ctx.fillRect(r, -6, (x4 - r) - r, 12);
        ctx.fillRect(x4 + r, -6, x_end - (x4 + r), 12); // Long handle extension

        // Marker positions along the wand body
        let marker_coords_m = [
            { x: 0, y: -0.25 },
            { x: 0, y: 0 },
            { x: 0, y: 0.125 },
            { x: 0.2, y: 0 }
        ];

        for (let m of marker_coords_m) {
            let mx = m.x * scale_m;
            let my = m.y * scale_m;
            ctx.beginPath();
            ctx.arc(mx, my, 14, 0, Math.PI * 2);
            ctx.fillStyle = '#ddd';
            ctx.fill();
            ctx.lineWidth = 2.5;
            ctx.strokeStyle = '#000';
            ctx.stroke();
        }
        ctx.restore();

        // 5. Draw LaTeX T_1 & T_2 equations above/below rays
        if (params.t_labels_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.t_labels_opacity;

            let mid_ray1 = { x: (ray1_start.x + wand_pos.x) / 2, y: (ray1_start.y + wand_pos.y) / 2 };
            let mid_ray2 = { x: (ray2_start.x + wand_pos.x) / 2, y: (ray2_start.y + wand_pos.y) / 2 };

            // Render T_1 midway above top ray
            ctx.save();
            ctx.translate(mid_ray1.x, mid_ray1.y - 35);
            ctx.scale(1.6 * (1 / math_scale()), 1.6 * (1 / math_scale()));
            ctx.drawImage(eq_T1, -eq_T1.width / 2, -eq_T1.height / 2);
            ctx.restore();

            // Render T_2 midway below bottom ray
            ctx.save();
            ctx.translate(mid_ray2.x, mid_ray2.y + 35);
            ctx.scale(1.6 * (1 / math_scale()), 1.6 * (1 / math_scale()));
            ctx.drawImage(eq_T2, -eq_T2.width / 2, -eq_T2.height / 2);
            ctx.restore();

            ctx.restore();
        }

        // 6. Draw "Find with PnP" callout with pointing arrows
        if (params.pnp_text_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.pnp_text_opacity;

            let pnp_x = wand_pos.x - 200;
            let pnp_y = center_y;
            ctx.font = 'bold 26px sans-serif';
            ctx.fillStyle = '#000';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('Find with PnP', pnp_x, pnp_y);

            // Point upwards to upper ray (T_1)
            let frac1 = (pnp_x - ray1_start.x) / (wand_pos.x - ray1_start.x);
            let target_ray1_y = ray1_start.y + (wand_pos.y - ray1_start.y) * frac1;
            drawArrowPos(ctx, { x: pnp_x, y: pnp_y - 20 }, { x: pnp_x, y: target_ray1_y + 8 }, 3, 11, false);

            // Point downwards to lower ray (T_2)
            let frac2 = (pnp_x - ray2_start.x) / (wand_pos.x - ray2_start.x);
            let target_ray2_y = ray2_start.y + (wand_pos.y - ray2_start.y) * frac2;
            drawArrowPos(ctx, { x: pnp_x, y: pnp_y + 20 }, { x: pnp_x, y: target_ray2_y - 8 }, 3, 11, false);

            ctx.restore();
        }

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title, tilted 0.5-opacity cameras, and the T-wand
    vid.add_transition(['title', 'relative_scene'], t, 0.8, { opacity: 1 });
    t += 0.8 + pause;

    // 2. Fade in the rays, T_1 and T_2 LaTeX labels, and "Find with PnP" text with pointing arrows
    vid.add_transition(['relative_scene'], t, 0.8, { rays_opacity: 1, t_labels_opacity: 1, pnp_text_opacity: 1 });
    t += 0.8 + pause;

    // 3. Fade out "Find with PnP" text immediately before fading in the purple text
    vid.add_transition(['relative_scene'], t, 0.5, { pnp_text_opacity: 0 });
    t += 0.5 + 0.3;

    // 4. Fade in purple baseline, T_2 T_1^-1 attached LaTeX label, and two-line "Find relative\ntransform matrix" callout
    vid.add_transition(['relative_scene'], t, 0.8, { purple_line_opacity: 1 });
    t += 0.8 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part7_bundle(canvas) {
    let vid = new Timeline();
    vid.set_name('part7_bundle');

    vid.add_object('title', { opacity: 0, text: 'Bundle Adjustment' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);
    let margin_x = 30;
    let avail_w = canvas.width - 2 * margin_x;
    let box_w = avail_w / 6.5;
    let box_h = box_w * 0.52;
    let font_sz = Math.max(14, Math.round(box_w * 0.115));

    // Align left border of leftmost box exactly with title (x = 30) and rightmost box symmetrically
    let start_x = margin_x + box_w / 2;
    let end_x = canvas.width - margin_x - box_w / 2;
    let col_step = (end_x - start_x) / 4;
    let col_x = (i) => start_x + i * col_step;

    let row_y0 = bg._top + bg._height * 0.22;
    let row_y1 = bg._top + bg._height * 0.65;
    let row_y = (r) => r === 0 ? row_y0 : row_y1;

    let boxes = {
        box_wand_3d: new DiagramBox({
            text: '3D Wand\nPoints',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            position: { x: col_x(0), y: row_y(0) }
        }),
        box_shift_wand: new DiagramBox({
            text: 'Shift By\nWand Position',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            background_color: '#000000',
            text_color: '#ffffff',
            position: { x: col_x(1), y: row_y(0) }
        }),
        box_shift_cam: new DiagramBox({
            text: 'Shift By\nCamera Position',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            background_color: '#000000',
            text_color: '#ffffff',
            position: { x: col_x(2), y: row_y(0) }
        }),
        box_project: new DiagramBox({
            text: 'Project',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            background_color: '#000000',
            text_color: '#ffffff',
            position: { x: col_x(3), y: row_y(0) }
        }),
        box_points_2d: new DiagramBox({
            text: '2D Points\n(Circles)',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            position: { x: col_x(4), y: row_y(0) }
        }),
        box_wand_pos: new DiagramBox({
            text: 'Wand\nPositions',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            position: { x: col_x(1), y: row_y(1) }
        }),
        box_cam_pos: new DiagramBox({
            text: 'Camera\nPositions',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            position: { x: col_x(2), y: row_y(1) }
        }),
        box_cam_params: new DiagramBox({
            text: 'Camera\nParameters',
            width: box_w,
            height: box_h,
            font_size: font_sz,
            position: { x: col_x(3), y: row_y(1) }
        })
    };

    Object.entries(boxes).forEach(([name, box]) => {
        vid.add_object(name, { opacity: 0 }, (ctx, params) => {
            if (params.opacity <= 0) return;
            ctx.save();
            ctx.globalAlpha *= params.opacity;
            box.draw(ctx);
            ctx.restore();
        });
    });

    // Horizontal connecting arrows along Row 0 pointing from left to right, touching box edges
    vid.add_object('row0_arrows', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';

        for (let c = 0; c < 4; c++) {
            let start_p = { x: col_x(c) + box_w / 2, y: row_y(0) };
            let end_p = { x: col_x(c + 1) - box_w / 2, y: row_y(0) };
            drawArrowPos(ctx, start_p, end_p, 3.5, 14, false);
        }
        ctx.restore();
    });

    // Upward pointing arrow from Wand Positions, touching box borders
    vid.add_object('arrow_wand_pos', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        let start_p = { x: col_x(1), y: row_y(1) - box_h / 2 };
        let end_p = { x: col_x(1), y: row_y(0) + box_h / 2 };
        drawArrowPos(ctx, start_p, end_p, 3.5, 14, false);
        ctx.restore();
    });

    // Upward pointing arrow from Camera Positions, touching box borders
    vid.add_object('arrow_cam_pos', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        let start_p = { x: col_x(2), y: row_y(1) - box_h / 2 };
        let end_p = { x: col_x(2), y: row_y(0) + box_h / 2 };
        drawArrowPos(ctx, start_p, end_p, 3.5, 14, false);
        ctx.restore();
    });

    // Upward pointing arrow from Camera Parameters, touching box borders
    vid.add_object('arrow_cam_params', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000';
        ctx.fillStyle = '#000';
        let start_p = { x: col_x(3), y: row_y(1) - box_h / 2 };
        let end_p = { x: col_x(3), y: row_y(0) + box_h / 2 };
        drawArrowPos(ctx, start_p, end_p, 3.5, 14, false);
        ctx.restore();
    });

    // Red dashed highlight boxes and labels
    vid.add_object('highlights', { opacity: 1, highlight1_opacity: 0, highlight2_opacity: 0 }, (ctx, params) => {
        if (params.highlight1_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.highlight1_opacity;
            ctx.strokeStyle = '#dd1111';
            ctx.lineWidth = 3.5;
            ctx.setLineDash([10, 7]);
            let bx1 = col_x(1) - box_w / 2 - 12;
            let bx2 = col_x(2) + box_w / 2 + 12;
            let by1 = row_y(1) - box_h / 2 - 12;
            let by2 = row_y(1) + box_h / 2 + 12;
            ctx.strokeRect(bx1, by1, bx2 - bx1, by2 - by1);
            ctx.setLineDash([]);

            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 26px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillText('Guess using PnP', (bx1 + bx2) / 2, by2 + 15);
            ctx.restore();
        }

        if (params.highlight2_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.highlight2_opacity;
            ctx.strokeStyle = '#dd1111';
            ctx.lineWidth = 3.5;
            ctx.setLineDash([10, 7]);
            let cx1 = col_x(3) - box_w / 2 - 12;
            let cx2 = col_x(3) + box_w / 2 + 12;
            let cy1 = row_y(1) - box_h / 2 - 12;
            let cy2 = row_y(1) + box_h / 2 + 12;
            ctx.strokeRect(cx1, cy1, cx2 - cx1, cy2 - cy1);
            ctx.setLineDash([]);

            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 24px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillText('Guess from checkerboard calibration', (cx1 + cx2) / 2, cy2 + 15);
            ctx.restore();
        }
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title
    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. On the first row, on the left, fade in "3D Wand Points" box
    vid.add_transition(['box_wand_3d'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 3. On the first row, on the right, fade in "2D Points (Circles)" box
    vid.add_transition(['box_points_2d'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 4. Fade in middle 3 boxes between them with connecting arrows from left to right
    vid.add_transition(['box_shift_wand', 'box_shift_cam', 'box_project', 'row0_arrows'], t, 0.8, { opacity: 1 });
    t += 0.8 + pause;

    // 5. Fade in "Wand Positions" box below the other wand positions box with arrow pointing up
    vid.add_transition(['box_wand_pos', 'arrow_wand_pos'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 6. Fade in "Camera Positions" box to the right with similar arrow up
    vid.add_transition(['box_cam_pos', 'arrow_cam_pos'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 7. Highlight both "Positions" boxes with red dashed box and label "Guess using PnP"
    vid.add_transition(['highlights'], t, 0.5, { highlight1_opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 8. Show third box while fading out highlight
    vid.add_transition(['box_cam_params', 'arrow_cam_params'], t, 0.5, { opacity: 1 });
    vid.add_transition(['highlights'], t, 0.5, { highlight1_opacity: 0 });
    t += 0.5 + pause;

    // 9. Fade in highlight for third box ("Guess from checkerboard calibration")
    vid.add_transition(['highlights'], t, 0.5, { highlight2_opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 10. Fade out that highlight
    vid.add_transition(['highlights'], t, 0.5, { highlight2_opacity: 0 });
    t += 0.5 + pause + 1.0;

    vid.set_duration(t);
    return vid;
}

export function part9_matching(canvas) {
    let vid = new Timeline();
    vid.set_name('part9_matching');

    vid.add_object('title', { opacity: 0, text: 'Point Matching' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);
    let margin = 30;
    let gap = 70;
    let frame_w = (canvas.width - 2 * margin - gap) / 2;
    let frame_h = Math.min(frame_w * 0.65, bg._height * 0.75);
    let frame_y = bg._top + 20;
    let left_x = margin;
    let right_x = margin + frame_w + gap;
    let cam3_x = right_x + frame_w + gap; // Third camera to the right before sliding

    vid.add_object('frames', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#000000';
        ctx.fillRect(left_x, frame_y, frame_w, frame_h);
        ctx.fillRect(right_x, frame_y, frame_w, frame_h);
        ctx.fillRect(cam3_x, frame_y, frame_w, frame_h);

        ctx.strokeStyle = '#555555';
        ctx.lineWidth = 2;
        ctx.strokeRect(left_x, frame_y, frame_w, frame_h);
        ctx.strokeRect(right_x, frame_y, frame_w, frame_h);
        ctx.strokeRect(cam3_x, frame_y, frame_w, frame_h);
        ctx.restore();
    });

    // 40 degrees relative to +x axis
    let angle = 40 * (Math.PI / 180);
    let dir_x = Math.cos(angle);
    let dir_y = -Math.sin(angle);
    let anchor_x = right_x + frame_w * 0.5;
    let anchor_y = frame_y + frame_h * 0.55;

    let left_pts = [];
    let right_pts = [];
    let cam3_pts = [];

    // 3 points lying directly on the epipolar line in the right image
    let d_vals = [-0.32 * frame_w, 0.05 * frame_w, 0.35 * frame_w];
    d_vals.forEach((d) => {
        right_pts.push({ x: anchor_x + d * dir_x, y: anchor_y + d * dir_y });
    });

    // Distinct layout for left frame (idx 1 is central point)
    left_pts.push({ x: left_x + frame_w * 0.20, y: frame_y + frame_h * 0.80 });
    left_pts.push({ x: left_x + frame_w * 0.48, y: frame_y + frame_h * 0.52 }); // Central point
    left_pts.push({ x: left_x + frame_w * 0.70, y: frame_y + frame_h * 0.20 });

    // Remaining 9 points for left and right frames with clearly distinct random layouts
    let left_extra_uvs = [
        { u: 0.15, v: 0.35 },
        { u: 0.30, v: 0.15 },
        { u: 0.55, v: 0.30 },
        { u: 0.85, v: 0.50 },
        { u: 0.75, v: 0.85 },
        { u: 0.50, v: 0.78 },
        { u: 0.32, v: 0.65 },
        { u: 0.12, v: 0.58 },
        { u: 0.82, v: 0.15 }
    ];

    let right_extra_uvs = [
        { u: 0.28, v: 0.22 },
        { u: 0.42, v: 0.32 },
        { u: 0.68, v: 0.45 },
        { u: 0.90, v: 0.70 },
        { u: 0.60, v: 0.82 },
        { u: 0.38, v: 0.88 },
        { u: 0.18, v: 0.65 },
        { u: 0.10, v: 0.42 },
        { u: 0.85, v: 0.32 }
    ];

    for (let i = 0; i < 9; i++) {
        left_pts.push({ x: left_x + left_extra_uvs[i].u * frame_w, y: frame_y + left_extra_uvs[i].v * frame_h });
        right_pts.push({ x: right_x + right_extra_uvs[i].u * frame_w, y: frame_y + right_extra_uvs[i].v * frame_h });
    }

    // 5 random points for third camera frame
    cam3_pts.push({ x: cam3_x + frame_w * 0.65, y: frame_y + frame_h * 0.45 }); // Valid match point
    cam3_pts.push({ x: cam3_x + frame_w * 0.15, y: frame_y + frame_h * 0.25 });
    cam3_pts.push({ x: cam3_x + frame_w * 0.82, y: frame_y + frame_h * 0.75 });
    cam3_pts.push({ x: cam3_x + frame_w * 0.50, y: frame_y + frame_h * 0.85 });
    cam3_pts.push({ x: cam3_x + frame_w * 0.85, y: frame_y + frame_h * 0.20 });

    let dark_spot = { x: cam3_x + frame_w * 0.35, y: frame_y + frame_h * 0.60 }; // Failed match dark spot

    vid.add_object('points', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#ffffff';
        for (let i = 0; i < left_pts.length; i++) {
            ctx.beginPath();
            ctx.arc(left_pts[i].x, left_pts[i].y, 4.5, 0, 2 * Math.PI);
            ctx.fill();
            ctx.beginPath();
            ctx.arc(right_pts[i].x, right_pts[i].y, 4.5, 0, 2 * Math.PI);
            ctx.fill();
        }
        for (let i = 0; i < cam3_pts.length; i++) {
            ctx.beginPath();
            ctx.arc(cam3_pts[i].x, cam3_pts[i].y, 4.5, 0, 2 * Math.PI);
            ctx.fill();
        }
        ctx.restore();
    });

    // All arrows except central point (index 1), with doubled arrowhead size (20)
    vid.add_object('arrows_other', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#dd1111';
        ctx.fillStyle = '#dd1111';
        for (let i = 0; i < left_pts.length; i++) {
            if (i === 1) continue;
            drawArrowPos(ctx, left_pts[i], right_pts[i], 2.5, 20, false);
        }
        ctx.restore();
    });

    // Arrow for central point (index 1), with doubled arrowhead size (20)
    vid.add_object('arrow_central', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#dd1111';
        ctx.fillStyle = '#dd1111';
        drawArrowPos(ctx, left_pts[1], right_pts[1], 2.5, 20, false);
        ctx.restore();
    });

    // Stroked circle around central point in left frame
    vid.add_object('left_highlight', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#dd1111';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.arc(left_pts[1].x, left_pts[1].y, 18, 0, 2 * Math.PI);
        ctx.stroke();
        ctx.restore();
    });

    // Red epipolar line in right frame
    vid.add_object('epipolar_line', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.beginPath();
        ctx.rect(right_x, frame_y, frame_w, frame_h);
        ctx.clip();

        ctx.strokeStyle = '#dd1111';
        ctx.lineWidth = 2.5;
        let len = frame_w * 2;
        ctx.beginPath();
        ctx.moveTo(anchor_x - len * dir_x, anchor_y - len * dir_y);
        ctx.lineTo(anchor_x + len * dir_x, anchor_y + len * dir_y);
        ctx.stroke();
        ctx.restore();
    });

    // Green circled points on epipolar line in right image (Top, Mid, Low)
    let draw_green_circle = (ctx, pt, opacity, shift_x) => {
        if (opacity <= 0) return;
        ctx.save();
        ctx.translate(shift_x || 0, 0);
        ctx.globalAlpha *= opacity;
        ctx.strokeStyle = '#00cc44';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.arc(pt.x, pt.y, 18, 0, 2 * Math.PI);
        ctx.stroke();
        ctx.restore();
    };

    vid.add_object('right_hl_low', { opacity: 0, shift_x: 0 }, (ctx, p) => draw_green_circle(ctx, right_pts[0], p.opacity, p.shift_x));
    vid.add_object('right_hl_mid', { opacity: 0, shift_x: 0 }, (ctx, p) => draw_green_circle(ctx, right_pts[1], p.opacity, p.shift_x));
    vid.add_object('right_hl_top', { opacity: 0, shift_x: 0 }, (ctx, p) => draw_green_circle(ctx, right_pts[2], p.opacity, p.shift_x));

    // Triangulate DiagramBox centered with Frame 2, 10px lower vertically
    let box_w = 160;
    let box_h = 58;
    let box_x = right_x + frame_w / 2;
    let box_y = frame_y + frame_h + 48;
    let tri_box = new DiagramBox({
        text: 'Triangulate',
        width: box_w,
        height: box_h,
        font_size: 20,
        position: { x: box_x, y: box_y }
    });

    vid.add_object('triangulate_box', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        tri_box.draw(ctx);
        ctx.restore();
    });

    // Right-angled orthogonal blue arrow from left central point down and turning right into left-middle of Triangulate box
    vid.add_object('tri_arrow_left', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#0055dd';
        ctx.fillStyle = '#0055dd';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(left_pts[1].x, left_pts[1].y);
        ctx.lineTo(left_pts[1].x, box_y);
        ctx.stroke();
        drawArrowPos(ctx, { x: left_pts[1].x, y: box_y }, { x: box_x - box_w / 2, y: box_y }, 3, 14, false);
        ctx.restore();
    });

    // Arrow from top most green circled point in right frame into top-middle of Triangulate box (Blue)
    vid.add_object('tri_arrow_right_top', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#0055dd';
        ctx.fillStyle = '#0055dd';
        drawArrowPos(ctx, right_pts[2], { x: box_x, y: box_y - box_h / 2 }, 3, 14, false);
        ctx.restore();
    });

    // Arrow from second (middle) green circled point into top-middle of Triangulate box (Blue)
    vid.add_object('tri_arrow_right_mid', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#0055dd';
        ctx.fillStyle = '#0055dd';
        drawArrowPos(ctx, right_pts[1], { x: box_x, y: box_y - box_h / 2 }, 3, 14, false);
        ctx.restore();
    });

    // Verification arrow out of right side of Triangulation box to dark spot in Frame 3 (Blue)
    vid.add_object('out_arrow_1', { opacity: 0, shift_x: 0, progress_x: 0, progress_y: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#0055dd';
        ctx.fillStyle = '#0055dd';
        ctx.lineWidth = 3;

        let start = { x: box_x + box_w / 2, y: box_y };
        let corner = { x: dark_spot.x, y: box_y };
        let end = { x: dark_spot.x, y: dark_spot.y + 20 };

        if (params.progress_x > 0 && params.progress_y <= 0) {
            let cur_x = start.x + (corner.x - start.x) * params.progress_x;
            drawArrowPos(ctx, start, { x: cur_x, y: start.y }, 3, 14, false);
        } else if (params.progress_y > 0) {
            ctx.beginPath();
            ctx.moveTo(start.x, start.y);
            ctx.lineTo(corner.x, corner.y);
            ctx.stroke();

            let cur_y = corner.y + (end.y - corner.y) * params.progress_y;
            drawArrowPos(ctx, corner, { x: corner.x, y: cur_y }, 3, 14, false);
        }
        ctx.restore();
    });

    // Red circle around dark spot in Frame 3 representing failed match
    vid.add_object('dark_spot_circle', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#dd1111';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.arc(dark_spot.x, dark_spot.y, 18, 0, 2 * Math.PI);
        ctx.stroke();
        ctx.restore();
    });

    // Verification arrow out of right side to valid matched point in Frame 3 (Blue)
    vid.add_object('out_arrow_2', { opacity: 0, shift_x: 0, progress_x: 0, progress_y: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#0055dd';
        ctx.fillStyle = '#0055dd';
        ctx.lineWidth = 3;

        let start = { x: box_x + box_w / 2, y: box_y };
        let corner = { x: cam3_pts[0].x, y: box_y };
        let end = { x: cam3_pts[0].x, y: cam3_pts[0].y + 20 };

        if (params.progress_x > 0 && params.progress_y <= 0) {
            let cur_x = start.x + (corner.x - start.x) * params.progress_x;
            drawArrowPos(ctx, start, { x: cur_x, y: start.y }, 3, 14, false);
        } else if (params.progress_y > 0) {
            ctx.beginPath();
            ctx.moveTo(start.x, start.y);
            ctx.lineTo(corner.x, corner.y);
            ctx.stroke();

            let cur_y = corner.y + (end.y - corner.y) * params.progress_y;
            drawArrowPos(ctx, corner, { x: corner.x, y: cur_y }, 3, 14, false);
        }
        ctx.restore();
    });

    // Green circle around valid matched point in Frame 3 representing successful match
    vid.add_object('valid_spot_circle', { opacity: 0, shift_x: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.translate(params.shift_x || 0, 0);
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#00cc44';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.arc(cam3_pts[0].x, cam3_pts[0].y, 18, 0, 2 * Math.PI);
        ctx.stroke();
        ctx.restore();
    });

    let all_scene_objs = [
        'frames', 'points', 'arrows_other', 'arrow_central', 'left_highlight',
        'epipolar_line', 'right_hl_low', 'right_hl_mid', 'right_hl_top',
        'triangulate_box', 'tri_arrow_left', 'tri_arrow_right_top', 'tri_arrow_right_mid',
        'out_arrow_1', 'dark_spot_circle', 'out_arrow_2', 'valid_spot_circle'
    ];

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and side-by-side frames 1 and 2
    vid.add_transition(['title', 'frames'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. Fade in set of 12 points into each frame
    vid.add_transition(['points'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 3. Fade in red arrows from each point in left frame to corresponding point in right frame
    vid.add_transition(['arrows_other', 'arrow_central'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 4. Fade out all but central arrow
    vid.add_transition(['arrows_other'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    // 5. Fade out remaining central arrow
    vid.add_transition(['arrow_central'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    // 6. Draw solid stroked circle around central point in left frame
    vid.add_transition(['left_highlight'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 7. Draw red epipolar line in right image at 40 degrees
    vid.add_transition(['epipolar_line'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 8. Fade in green stroked circles around each of the 3 points on epipolar line in right image
    vid.add_transition(['right_hl_low', 'right_hl_mid', 'right_hl_top'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 9. Fade out all but one of green highlighted points (keep the top most one)
    vid.add_transition(['right_hl_low', 'right_hl_mid'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    // 10. Fade in Triangulate box and arrows coming from the two circled points into the top corners
    vid.add_transition(['triangulate_box', 'tri_arrow_left', 'tri_arrow_right_top'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause;

    // 11. Slide scene left revealing Camera 3 while simultaneously drawing arrow out right side of Triangulate box
    vid.add_transition(all_scene_objs, t, 1.2, { shift_x: -(frame_w + gap) });
    vid.add_transition(['out_arrow_1'], t, 1.2, { opacity: 1, progress_x: 1, progress_y: 0 });
    t += 1.2 + pause;

    // 12. Arrow turns up, points to dark spot in Frame 3, and circles it in red (failed match)
    vid.add_transition(['out_arrow_1'], t, 0.5, { progress_y: 1 });
    vid.add_transition(['dark_spot_circle'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 1.0;

    // 13. Fade out arrow from green circle, top green circle itself, outgoing arrow, and dark spot circle
    vid.add_transition(['tri_arrow_right_top', 'right_hl_top', 'out_arrow_1', 'dark_spot_circle'], t, 0.5, { opacity: 0 });
    t += 0.5 + pause;

    // 14. Immediately re-fade in second green point in line and arrow into Triangulate box
    vid.add_transition(['right_hl_mid', 'tri_arrow_right_mid'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 15. Animate new arrow out of Triangulate box and turning up to circle valid point in Frame 3 in green
    vid.add_transition(['out_arrow_2'], t, 0.6, { opacity: 1, progress_x: 1, progress_y: 0 });
    t += 0.6 + 0.1;

    vid.add_transition(['out_arrow_2'], t, 0.5, { progress_y: 1 });
    vid.add_transition(['valid_spot_circle'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part9_triangulation(canvas) {
    let vid = new Timeline();
    vid.set_name('part9_triangulation');

    vid.add_object('title', { opacity: 0, text: 'Triangulation' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);
    let box_w = 180;
    let box_h = 80;
    let font_sz = 18;

    // 4 columns across width: leftmost edge at x = 30, rightmost wire extent at canvas.width - 30
    let col_0_x = 30 + box_w / 2;
    let col_3_x = canvas.width - 65 - box_w / 2;
    let col_x = (c) => col_0_x + (c * (col_3_x - col_0_x) / 3);

    let row_y = (r) => bg._top + 85 + r * 145;
    let mid_y = (row_y(0) + row_y(1)) / 2;

    let box_point_3d = new DiagramBox({
        text: '3D Point\n(Guess)',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        position: { x: col_x(0), y: mid_y }
    });

    let box_add_cam1 = new DiagramBox({
        text: 'Add Camera 1\nPosition',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        background_color: '#000000',
        text_color: '#ffffff',
        position: { x: col_x(1), y: row_y(0) }
    });

    let box_add_cam2 = new DiagramBox({
        text: 'Add Camera 2\nPosition',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        background_color: '#000000',
        text_color: '#ffffff',
        position: { x: col_x(1), y: row_y(1) }
    });

    let box_proj_cam1 = new DiagramBox({
        text: 'Project\nCamera 1',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        background_color: '#000000',
        text_color: '#ffffff',
        position: { x: col_x(2), y: row_y(0) }
    });

    let box_proj_cam2 = new DiagramBox({
        text: 'Project\nCamera 2',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        background_color: '#000000',
        text_color: '#ffffff',
        position: { x: col_x(2), y: row_y(1) }
    });

    let box_point_2d_1 = new DiagramBox({
        text: '2D Point #1',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        position: { x: col_x(3), y: row_y(0) }
    });

    let box_point_2d_2 = new DiagramBox({
        text: '2D Point #2',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        position: { x: col_x(3), y: row_y(1) }
    });

    vid.add_object('col0_box', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        box_point_3d.draw(ctx);
        ctx.restore();
    });

    vid.add_object('col0_arrows', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000000';
        ctx.fillStyle = '#000000';
        drawArrowPos(ctx, { x: col_x(0) + box_w / 2, y: mid_y }, { x: col_x(1) - box_w / 2, y: row_y(0) }, 2.5, 12, false);
        drawArrowPos(ctx, { x: col_x(0) + box_w / 2, y: mid_y }, { x: col_x(1) - box_w / 2, y: row_y(1) }, 2.5, 12, false);
        ctx.restore();
    });

    vid.add_object('col1_boxes', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        box_add_cam1.draw(ctx);
        box_add_cam2.draw(ctx);
        ctx.restore();
    });

    vid.add_object('col1_arrows', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000000';
        ctx.fillStyle = '#000000';
        drawArrowPos(ctx, { x: col_x(1) + box_w / 2, y: row_y(0) }, { x: col_x(2) - box_w / 2, y: row_y(0) }, 2.5, 12, false);
        drawArrowPos(ctx, { x: col_x(1) + box_w / 2, y: row_y(1) }, { x: col_x(2) - box_w / 2, y: row_y(1) }, 2.5, 12, false);
        ctx.restore();
    });

    vid.add_object('col2_boxes', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        box_proj_cam1.draw(ctx);
        box_proj_cam2.draw(ctx);
        ctx.restore();
    });

    vid.add_object('col2_arrows', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000000';
        ctx.fillStyle = '#000000';
        drawArrowPos(ctx, { x: col_x(2) + box_w / 2, y: row_y(0) }, { x: col_x(3) - box_w / 2, y: row_y(0) }, 2.5, 12, false);
        drawArrowPos(ctx, { x: col_x(2) + box_w / 2, y: row_y(1) }, { x: col_x(3) - box_w / 2, y: row_y(1) }, 2.5, 12, false);
        ctx.restore();
    });

    vid.add_object('col3_boxes', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        box_point_2d_1.draw(ctx);
        box_point_2d_2.draw(ctx);
        ctx.restore();
    });

    vid.add_object('feedback_loop', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000000';
        ctx.fillStyle = '#000000';
        ctx.lineWidth = 2.5;

        let wire_y_2 = row_y(1) + box_h / 2 + 65;
        let rx2 = col_x(3) + box_w / 2 + 20;
        let lx2 = col_x(0) + 15;
        ctx.beginPath();
        ctx.moveTo(col_x(3) + box_w / 2, row_y(1));
        ctx.lineTo(rx2, row_y(1));
        ctx.lineTo(rx2, wire_y_2);
        ctx.lineTo(lx2, wire_y_2);
        ctx.stroke();
        drawArrowPos(ctx, { x: lx2, y: wire_y_2 }, { x: lx2, y: mid_y + box_h / 2 }, 2.5, 12, false);

        let wire_y_1 = row_y(1) + box_h / 2 + 80;
        let rx1 = col_x(3) + box_w / 2 + 35;
        let lx1 = col_x(0) - 15;
        ctx.beginPath();
        ctx.moveTo(col_x(3) + box_w / 2, row_y(0));
        ctx.lineTo(rx1, row_y(0));
        ctx.lineTo(rx1, wire_y_1);
        ctx.lineTo(lx1, wire_y_1);
        ctx.stroke();
        drawArrowPos(ctx, { x: lx1, y: wire_y_1 }, { x: lx1, y: mid_y + box_h / 2 }, 2.5, 12, false);

        ctx.font = '20px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        ctx.fillText('Adjust 3d Point', (col_x(0) + col_x(3)) / 2, wire_y_1 + 14);
        ctx.restore();
    });

    vid.add_object('red_highlight', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        let hl_left = col_x(1) - box_w / 2 - 15;
        let hl_right = col_x(2) + box_w / 2 + 15;
        let hl_top = row_y(0) - box_h / 2 - 15;
        let hl_bot = row_y(1) + box_h / 2 + 15;

        ctx.strokeStyle = '#dd1111';
        ctx.lineWidth = 2.5;
        ctx.setLineDash([8, 6]);
        ctx.strokeRect(hl_left, hl_top, hl_right - hl_left, hl_bot - hl_top);
        ctx.setLineDash([]);

        ctx.fillStyle = '#dd1111';
        ctx.font = '20px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        ctx.fillText('Found during wanding', (hl_left + hl_right) / 2, hl_bot + 8);
        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title
    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 2. On the left, fade in "3D Point (Guess)" box
    vid.add_transition(['col0_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 3. Fade in "Add Camera 1/2 Position" black boxes to the right with connecting arrows
    vid.add_transition(['col1_boxes', 'col0_arrows'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 4. Fade in "Project Camera 1/2" black boxes with connecting arrows
    vid.add_transition(['col2_boxes', 'col1_arrows'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 5. Fade in "2D Point #1/2" regular boxes on the right with connecting arrows
    vid.add_transition(['col3_boxes', 'col2_arrows'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 6. Draw feedback wire pair and label "Adjust 3d Point"
    vid.add_transition(['feedback_loop'], t, 0.8, { opacity: 1 });
    t += 0.8 + pause + 0.5;

    // 7. Fade in red dashed highlight around all black boxes with label "Found during wanding"
    vid.add_transition(['red_highlight'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part10_scaling(canvas) {
    let vid = new Timeline();
    vid.set_name('part10_scaling');

    vid.add_object('title', { opacity: 0, text: 'Scaling Matching' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);

    // Initial 2-frame view dimensions (same style as part9_matching)
    let margin = 30;
    let gap_init = 70;
    let w_init = (canvas.width - 2 * margin - gap_init) / 2;
    let h_init = Math.min(w_init * 0.65, bg._height * 0.75);
    let y_init_start = bg._top + 20;

    // Final 8-frame view dimensions (2 rows of 4 cameras each)
    let gap_x_full = 25;
    let gap_y_full = 30;
    let w_full = (canvas.width - 2 * margin - 3 * gap_x_full) / 4;
    let h_full = Math.min(w_full * 0.68, (bg._height - gap_y_full) / 2);
    let y_full_start = bg._top + (bg._height - (2 * h_full + gap_y_full)) / 2;

    // Helper to compute position & size of camera i (0..7) at interpolation factor z (0=initial, 1=final)
    let get_cam_box = (i, z) => {
        let c = i % 4;
        let r = Math.floor(i / 4);

        let x_0 = margin + c * (w_init + gap_init);
        let y_0 = y_init_start + r * (h_init + gap_y_full * (w_init / w_full));

        let x_1 = margin + c * (w_full + gap_x_full);
        let y_1 = y_full_start + r * (h_full + gap_y_full);

        let x = x_0 + (x_1 - x_0) * z;
        let y = y_0 + (y_1 - y_0) * z;
        let w = w_init + (w_full - w_init) * z;
        let h = h_init + (h_full - h_init) * z;

        return { x, y, w, h };
    };

    // Generate seeded random points in normalized (u, v) coordinates for all 8 cameras
    let all_cams_pts = [];
    let seeded_random = (seed) => {
        let x = Math.sin(seed++) * 10000;
        return x - Math.floor(x);
    };

    for (let i = 0; i < 8; i++) {
        let pts = [];
        if (i === 0) {
            pts.push({ u: 0.48, v: 0.52 }); // Central highlighted point
            for (let j = 1; j < 12; j++) {
                pts.push({
                    u: 0.12 + 0.76 * seeded_random(i * 100 + j * 13 + 7),
                    v: 0.12 + 0.76 * seeded_random(i * 100 + j * 19 + 11)
                });
            }
        } else {
            for (let j = 0; j < 12; j++) {
                pts.push({
                    u: 0.12 + 0.76 * seeded_random(i * 100 + j * 13 + 7),
                    v: 0.12 + 0.76 * seeded_random(i * 100 + j * 19 + 11)
                });
            }
        }
        all_cams_pts.push(pts);
    }

    vid.add_object('cameras_initial', { opacity: 0, zoom_out: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let z = params.zoom_out || 0;
        let dot_r = 4.5 - 1.5 * z;

        for (let i = 0; i < 2; i++) {
            let box = get_cam_box(i, z);
            ctx.fillStyle = '#000000';
            ctx.fillRect(box.x, box.y, box.w, box.h);

            ctx.strokeStyle = '#555555';
            ctx.lineWidth = 2;
            ctx.strokeRect(box.x, box.y, box.w, box.h);

            ctx.fillStyle = '#ffffff';
            let pts = all_cams_pts[i];
            for (let j = 0; j < pts.length; j++) {
                ctx.beginPath();
                ctx.arc(box.x + pts[j].u * box.w, box.y + pts[j].v * box.h, dot_r, 0, 2 * Math.PI);
                ctx.fill();
            }
        }
        ctx.restore();
    });

    vid.add_object('cameras_other', { opacity: 0, zoom_out: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let z = params.zoom_out || 0;
        let dot_r = 4.5 - 1.5 * z;

        for (let i = 2; i < 8; i++) {
            let box = get_cam_box(i, z);
            if (box.x > canvas.width + 200 || box.y > canvas.height + 200) continue;

            ctx.fillStyle = '#000000';
            ctx.fillRect(box.x, box.y, box.w, box.h);

            ctx.strokeStyle = '#555555';
            ctx.lineWidth = 2;
            ctx.strokeRect(box.x, box.y, box.w, box.h);

            ctx.fillStyle = '#ffffff';
            let pts = all_cams_pts[i];
            for (let j = 0; j < pts.length; j++) {
                ctx.beginPath();
                ctx.arc(box.x + pts[j].u * box.w, box.y + pts[j].v * box.h, dot_r, 0, 2 * Math.PI);
                ctx.fill();
            }
        }
        ctx.restore();
    });

    vid.add_object('highlight', { opacity: 0, zoom_out: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let z = params.zoom_out || 0;
        let box = get_cam_box(0, z);
        let pt = all_cams_pts[0][0];

        ctx.strokeStyle = '#dd1111';
        ctx.lineWidth = 3 - 0.5 * z;
        ctx.beginPath();
        ctx.arc(box.x + pt.u * box.w, box.y + pt.v * box.h, 18 - 6 * z, 0, 2 * Math.PI);
        ctx.stroke();
        ctx.restore();
    });

    // Epipolar line angles and anchors for cameras 1 through 7
    let epi_angles = [0, 40, -25, 60, 15, -45, 30, -10];
    let epi_anchors = [
        { u: 0.5, v: 0.5 },
        { u: 0.5, v: 0.55 },
        { u: 0.45, v: 0.50 },
        { u: 0.55, v: 0.45 },
        { u: 0.50, v: 0.60 },
        { u: 0.50, v: 0.50 },
        { u: 0.40, v: 0.55 },
        { u: 0.50, v: 0.45 }
    ];

    vid.add_object('epipolar_lines', { opacity: 0, zoom_out: 1 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let z = params.zoom_out || 1;

        for (let i = 1; i < 8; i++) {
            let box = get_cam_box(i, z);
            ctx.save();
            ctx.beginPath();
            ctx.rect(box.x, box.y, box.w, box.h);
            ctx.clip();

            let angle = epi_angles[i] * (Math.PI / 180);
            let dir_x = Math.cos(angle);
            let dir_y = -Math.sin(angle);
            let anchor_px = box.x + epi_anchors[i].u * box.w;
            let anchor_py = box.y + epi_anchors[i].v * box.h;

            ctx.strokeStyle = '#dd1111';
            ctx.lineWidth = 2.5;
            let len = box.w * 2;
            ctx.beginPath();
            ctx.moveTo(anchor_px - len * dir_x, anchor_py - len * dir_y);
            ctx.lineTo(anchor_px + len * dir_x, anchor_py + len * dir_y);
            ctx.stroke();
            ctx.restore();
        }
        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and initial two side-by-side frames with red circle around left central point
    vid.add_transition(['title', 'cameras_initial', 'highlight'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause + 0.8;

    // 2. Zoom out effect showing 2 rows of 4 cameras each, fading in the other 6 frames during the zoom
    vid.add_transition(['cameras_initial', 'cameras_other', 'highlight'], t, 1.2, { zoom_out: 1 });
    vid.add_transition(['cameras_other'], t, 1.2, { opacity: 1 });
    t += 1.2 + pause;

    // 3. Draw epipolar lines at random angles in all frames other than the first one
    vid.add_transition(['epipolar_lines'], t, 0.6, { opacity: 1, zoom_out: 1 });
    t += 0.6 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part10_rematching(canvas) {
    let vid = new Timeline();
    vid.set_name('part10_rematching');

    vid.add_object('title', { opacity: 0, text: 'Rematching' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);
    let margin = 30;
    let gap = 70;
    let frame_w = (canvas.width - 2 * margin - gap) / 2;
    let frame_h = Math.min(frame_w * 0.65, bg._height * 0.75);
    let frame_y = bg._top + 20;
    let left_x = margin;
    let right_x = margin + frame_w + gap;

    vid.add_object('frames', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#000000';
        ctx.fillRect(left_x, frame_y, frame_w, frame_h);
        ctx.fillRect(right_x, frame_y, frame_w, frame_h);

        ctx.strokeStyle = '#555555';
        ctx.lineWidth = 2;
        ctx.strokeRect(left_x, frame_y, frame_w, frame_h);
        ctx.strokeRect(right_x, frame_y, frame_w, frame_h);
        ctx.restore();
    });

    // Generate random points for both frames
    let left_pts = [];
    let right_pts = [];
    let seeded_random = (seed) => {
        let x = Math.sin(seed++) * 10000;
        return x - Math.floor(x);
    };

    // Target point for left frame at (0.65, 0.55), target for right frame at (0.35, 0.60)
    let target_left = { x: left_x + 0.65 * frame_w, y: frame_y + 0.55 * frame_h };
    let target_right = { x: right_x + 0.35 * frame_w, y: frame_y + 0.60 * frame_h };

    left_pts.push(target_left);
    right_pts.push(target_right);

    for (let i = 1; i < 12; i++) {
        left_pts.push({
            x: left_x + (0.15 + 0.75 * seeded_random(300 + i * 17)) * frame_w,
            y: frame_y + (0.15 + 0.75 * seeded_random(300 + i * 29)) * frame_h
        });
        right_pts.push({
            x: right_x + (0.15 + 0.75 * seeded_random(500 + i * 23)) * frame_w,
            y: frame_y + (0.15 + 0.75 * seeded_random(500 + i * 31)) * frame_h
        });
    }

    vid.add_object('points', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#ffffff';
        for (let i = 0; i < left_pts.length; i++) {
            ctx.beginPath();
            ctx.arc(left_pts[i].x, left_pts[i].y, 4.5, 0, 2 * Math.PI);
            ctx.fill();
            ctx.beginPath();
            ctx.arc(right_pts[i].x, right_pts[i].y, 4.5, 0, 2 * Math.PI);
            ctx.fill();
        }
        ctx.restore();
    });

    // "Predicted 3D\nPoint" box between the frames, shifted down vertically
    let box_w = 175;
    let box_h = 65;
    let box_x = margin + frame_w + gap / 2;
    let box_y = frame_y + frame_h + 45;
    let pred_box = new DiagramBox({
        text: 'Predicted 3D\nPoint',
        width: box_w,
        height: box_h,
        font_size: 19,
        position: { x: box_x, y: box_y }
    });

    vid.add_object('predicted_box', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        pred_box.draw(ctx);
        ctx.restore();
    });

    // Blue arrow left: comes out left side of box, turns up to point to target in Frame 1
    vid.add_object('arrow_left', { opacity: 0, progress_x: 0, progress_y: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#0055dd';
        ctx.fillStyle = '#0055dd';
        ctx.lineWidth = 3;

        let start = { x: box_x - box_w / 2, y: box_y };
        let corner = { x: target_left.x, y: box_y };
        let end = { x: target_left.x, y: target_left.y + 20 };

        if (params.progress_x > 0 && params.progress_y <= 0) {
            let cur_x = start.x + (corner.x - start.x) * params.progress_x;
            drawArrowPos(ctx, start, { x: cur_x, y: start.y }, 3, 14, false);
        } else if (params.progress_y > 0) {
            ctx.beginPath();
            ctx.moveTo(start.x, start.y);
            ctx.lineTo(corner.x, corner.y);
            ctx.stroke();

            let cur_y = corner.y + (end.y - corner.y) * params.progress_y;
            drawArrowPos(ctx, corner, { x: corner.x, y: cur_y }, 3, 14, false);
        }
        ctx.restore();
    });

    // Blue arrow right: comes out right side of box, turns up to point to target in Frame 2
    vid.add_object('arrow_right', { opacity: 0, progress_x: 0, progress_y: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#0055dd';
        ctx.fillStyle = '#0055dd';
        ctx.lineWidth = 3;

        let start = { x: box_x + box_w / 2, y: box_y };
        let corner = { x: target_right.x, y: box_y };
        let end = { x: target_right.x, y: target_right.y + 20 };

        if (params.progress_x > 0 && params.progress_y <= 0) {
            let cur_x = start.x + (corner.x - start.x) * params.progress_x;
            drawArrowPos(ctx, start, { x: cur_x, y: start.y }, 3, 14, false);
        } else if (params.progress_y > 0) {
            ctx.beginPath();
            ctx.moveTo(start.x, start.y);
            ctx.lineTo(corner.x, corner.y);
            ctx.stroke();

            let cur_y = corner.y + (end.y - corner.y) * params.progress_y;
            drawArrowPos(ctx, corner, { x: corner.x, y: cur_y }, 3, 14, false);
        }
        ctx.restore();
    });

    // Red circle highlights on the two matched circles immediately after arrows finish
    vid.add_object('red_circles', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#dd1111';
        ctx.lineWidth = 3;

        ctx.beginPath();
        ctx.arc(target_left.x, target_left.y, 18, 0, 2 * Math.PI);
        ctx.stroke();

        ctx.beginPath();
        ctx.arc(target_right.x, target_right.y, 18, 0, 2 * Math.PI);
        ctx.stroke();
        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Start with title, 2 side-by-side frames, and random white points in them
    vid.add_transition(['title', 'frames', 'points'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause;

    // 2. Fade in "Predicted 3D Point" box in between and shifted down
    vid.add_transition(['predicted_box'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 3. Show blue arrows going out left and right sides of the box
    vid.add_transition(['arrow_left', 'arrow_right'], t, 0.5, { opacity: 1, progress_x: 1, progress_y: 0 });
    t += 0.5 + 0.05;

    // 4. Arrows turn and go up to point to one of the circles in both frames
    vid.add_transition(['arrow_left', 'arrow_right'], t, 0.5, { progress_y: 1 });
    t += 0.5; // Immediately mark circles after animation finishes

    // 5. Immediately mark those two circles with red circle highlights
    vid.add_transition(['red_circles'], t, 0.35, { opacity: 1 });
    t += 0.35 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part11_simulation(canvas) {
    let vid = new Timeline();
    vid.set_name('part11_simulation');

    vid.add_object('title', { opacity: 0, text: 'Camera Simulation' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);
    let box_w = 180;
    let box_h = 80;
    let font_sz = 20;

    // 3 columns across slide width
    let col_0_x = 40 + box_w / 2;
    let col_2_x = canvas.width - 40 - box_w / 2;
    let col_1_x = (col_0_x + col_2_x) / 2;

    let row_0_y = bg._top + bg._height * 0.35;
    let row_1_y = row_0_y + 150;

    let box_real_cam = new DiagramBox({
        text: 'Real\nCamera',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        position: { x: col_0_x, y: row_0_y }
    });

    let box_img_proc = new DiagramBox({
        text: 'Image\nProcessor',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        position: { x: col_1_x, y: row_0_y }
    });

    let box_triangulation = new DiagramBox({
        text: 'Triangulation',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        position: { x: col_2_x, y: row_0_y }
    });

    let box_video_game = new DiagramBox({
        text: 'Video\nGame',
        width: box_w,
        height: box_h,
        font_size: font_sz,
        position: { x: col_0_x, y: row_1_y }
    });

    vid.add_object('box_real_cam', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        box_real_cam.draw(ctx);
        ctx.restore();
    });

    vid.add_object('arrow_real_cam', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000000';
        ctx.fillStyle = '#000000';
        drawArrowPos(ctx, { x: col_0_x + box_w / 2, y: row_0_y }, { x: col_1_x - box_w / 2, y: row_0_y }, 3, 14, false);
        ctx.restore();
    });

    vid.add_object('pipeline_rest', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        box_img_proc.draw(ctx);
        box_triangulation.draw(ctx);
        ctx.strokeStyle = '#000000';
        ctx.fillStyle = '#000000';
        drawArrowPos(ctx, { x: col_1_x + box_w / 2, y: row_0_y }, { x: col_2_x - box_w / 2, y: row_0_y }, 3, 14, false);
        ctx.restore();
    });

    vid.add_object('box_video_game', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        box_video_game.draw(ctx);
        ctx.restore();
    });

    vid.add_object('arrow_video_game', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#000000';
        ctx.fillStyle = '#000000';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(col_0_x + box_w / 2, row_1_y);
        ctx.lineTo(col_1_x, row_1_y);
        ctx.stroke();
        drawArrowPos(ctx, { x: col_1_x, y: row_1_y }, { x: col_1_x, y: row_0_y + box_h / 2 }, 3, 14, false);
        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title, 3 diagram boxes, and left-to-right arrows
    vid.add_transition(['title', 'box_real_cam', 'arrow_real_cam', 'pipeline_rest'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause + 1.0;

    // 2. Simultaneously fade "Real Camera" box and arrow to 50%, and fade in "Video Game" box with right-angle arrow
    vid.add_transition(['box_real_cam', 'arrow_real_cam'], t, 0.6, { opacity: 0.5 });
    vid.add_transition(['box_video_game', 'arrow_video_game'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part12_rigid_body(canvas) {
    let vid = new Timeline();
    vid.set_name('part12_rigid_body');

    vid.add_object('title', { opacity: 0, text: 'Finding Rigid Bodies' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);
    let margin = 30;
    let gap = 50;
    // Right 2/3, Left 1/3
    let total_w = canvas.width - 2 * margin - gap;
    let left_w = Math.min(total_w * 0.32, bg._height * 0.65);
    let left_x = margin;
    let left_y = bg._top + 50;

    let right_x = left_x + left_w + gap;
    let right_w = canvas.width - right_x - margin;
    let right_y = bg._top;
    let right_h = bg._height;

    // Pattern geometry (centered around origin)
    let R = left_w * 0.32;
    let p0 = { x: 0, y: -R };
    let p1 = { x: -R * Math.cos(Math.PI / 6), y: R * Math.sin(Math.PI / 6) };
    let p2 = { x: R * Math.cos(Math.PI / 6), y: R * Math.sin(Math.PI / 6) };
    let p3 = { x: p1.x * 0.5, y: p1.y * 0.5 }; // midway between p1 and origin (center of triangle)
    let raw_pts = [p0, p1, p2, p3];

    // Pattern points positioned inside the left square
    let pat_center = { x: left_x + left_w / 2, y: left_y + left_w / 2 };
    let pat_pts = raw_pts.map(p => ({ x: pat_center.x + p.x, y: pat_center.y + p.y }));

    // Point cloud data (30 points in right box)
    let cloud_pts = [];
    let seeded_random = (seed) => {
        let x = Math.sin(seed++) * 10000;
        return x - Math.floor(x);
    };

    // First 4 points are the rigged match with random rotation
    let rot_angle = 1.25; // around 71 degrees
    let cos_a = Math.cos(rot_angle);
    let sin_a = Math.sin(rot_angle);
    let cloud_center = { x: right_x + right_w * 0.52, y: right_y + right_h * 0.52 };

    for (let i = 0; i < 4; i++) {
        let rx = raw_pts[i].x * cos_a - raw_pts[i].y * sin_a;
        let ry = raw_pts[i].x * sin_a + raw_pts[i].y * cos_a;
        cloud_pts.push({ x: cloud_center.x + rx, y: cloud_center.y + ry });
    }

    // Generate 26 remaining random points across right box
    for (let i = 4; i < 30; i++) {
        cloud_pts.push({
            x: right_x + 30 + seeded_random(900 + i * 31) * (right_w - 60),
            y: right_y + 30 + seeded_random(900 + i * 47) * (right_h - 60)
        });
    }

    let rb_center = {
        x: (cloud_pts[0].x + cloud_pts[1].x + cloud_pts[2].x + cloud_pts[3].x) / 4,
        y: (cloud_pts[0].y + cloud_pts[1].y + cloud_pts[2].y + cloud_pts[3].y) / 4
    };
    let transform_rb = (p, z) => {
        if (!z) return p;
        let angle = -0.35 * z; // small rotation (~ -20 degrees) around rigid body centroid
        let cos = Math.cos(angle);
        let sin = Math.sin(angle);
        let dx = p.x - rb_center.x;
        let dy = p.y - rb_center.y;
        return {
            x: rb_center.x + (dx * cos - dy * sin) + 20 * z,
            y: rb_center.y + (dx * sin + dy * cos) - 6 * z
        };
    };

    // Draw point cloud box with dots (supports sliding/rotating and highlighting the 4 matched dots)
    vid.add_object('point_cloud', { opacity: 0, move_progress: 0, highlight_red: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        ctx.fillStyle = '#ffffff';
        ctx.fillRect(right_x, right_y, right_w, right_h);
        ctx.strokeStyle = '#000000';
        ctx.lineWidth = 2;
        ctx.strokeRect(right_x, right_y, right_w, right_h);

        for (let i = 0; i < cloud_pts.length; i++) {
            let pt = (i < 4) ? transform_rb(cloud_pts[i], params.move_progress || 0) : cloud_pts[i];
            ctx.fillStyle = (i < 4 && params.highlight_red > 0) ? '#dd1111' : '#0055dd';
            ctx.beginPath();
            ctx.arc(pt.x, pt.y, 5.5, 0, 2 * Math.PI);
            ctx.fill();
        }
        ctx.restore();
    });

    // Draw search pattern square with label and dots (expands down when slide_progress > 0)
    vid.add_object('search_pattern', { opacity: 0, slide_progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let z = params.slide_progress || 0;
        let cur_h = left_w + (right_y + right_h - left_y - left_w) * z;

        ctx.fillStyle = '#ffffff';
        ctx.fillRect(left_x, left_y, left_w, cur_h);
        ctx.strokeStyle = '#000000';
        ctx.lineWidth = 2;
        ctx.strokeRect(left_x, left_y, left_w, cur_h);

        ctx.fillStyle = '#000000';
        ctx.font = 'bold 22px sans-serif';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'bottom';
        ctx.fillText('Search Pattern', left_x + left_w / 2, left_y - 12);

        ctx.fillStyle = '#0055dd';
        for (let pt of pat_pts) {
            ctx.beginPath();
            ctx.arc(pt.x, pt.y, 5.5, 0, 2 * Math.PI);
            ctx.fill();
        }
        ctx.restore();
    });

    // 10 sets of 4 random target indices for brute force searching
    let bf_target_sets = [];
    for (let s = 0; s < 10; s++) {
        let targets = [];
        let iter = 0;
        while (targets.length < 4 && iter < 100) {
            iter++;
            let idx = Math.floor(seeded_random(1200 + s * 100 + iter * 19) * 26) + 4;
            if (!targets.includes(idx)) targets.push(idx);
        }
        bf_target_sets.push(targets);
    }

    vid.add_object('bf_arrows', { opacity: 0, step_idx: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let idx = Math.min(9, Math.max(0, Math.round(params.step_idx || 0)));
        let targets = bf_target_sets[idx];

        ctx.strokeStyle = '#dd6600';
        ctx.fillStyle = '#dd6600';
        for (let i = 0; i < 4; i++) {
            drawArrowPos(ctx, pat_pts[i], cloud_pts[targets[i]], 2.5, 14, false);
        }
        ctx.restore();
    });

    // Draw lines for the two pattern triangles in the left box
    // Triangle 1: Equilateral (0 -> 1 -> 2 -> 0)
    // Triangle 2: Further two points & midway point (0 -> 2 -> 3 -> 0)
    vid.add_object('pat_triangles', { opacity: 0, slide_progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Draw in-place inside the pattern box
        ctx.strokeStyle = '#00a844'; // green for equilateral
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.moveTo(pat_pts[0].x, pat_pts[0].y);
        ctx.lineTo(pat_pts[1].x, pat_pts[1].y);
        ctx.lineTo(pat_pts[2].x, pat_pts[2].y);
        ctx.closePath();
        ctx.stroke();

        ctx.strokeStyle = '#a200ff'; // purple for inner triangle
        ctx.beginPath();
        ctx.moveTo(pat_pts[0].x, pat_pts[0].y);
        ctx.lineTo(pat_pts[2].x, pat_pts[2].y);
        ctx.lineTo(pat_pts[3].x, pat_pts[3].y);
        ctx.closePath();
        ctx.stroke();

        // Slide animation below the rigid body box
        if (params.slide_progress > 0) {
            let z = params.slide_progress;
            let dest_y = left_y + left_w + (bg._top + bg._height - left_y - left_w) * 0.50;
            let dest_center_1 = { x: left_x + left_w * 0.30, y: dest_y };
            let dest_center_2 = { x: left_x + left_w * 0.70, y: dest_y };
            let scale_f = 0.55; // slightly scaled down to fit cleanly side-by-side underneath

            // Triangle 1 sliding copy
            ctx.save();
            ctx.strokeStyle = '#00a844';
            ctx.lineWidth = 2.5;
            ctx.beginPath();
            for (let i of [0, 1, 2]) {
                let cur_x = (pat_pts[i].x) * (1 - z) + (dest_center_1.x + raw_pts[i].x * scale_f) * z;
                let cur_y = (pat_pts[i].y) * (1 - z) + (dest_center_1.y + raw_pts[i].y * scale_f) * z;
                if (i === 0) ctx.moveTo(cur_x, cur_y); else ctx.lineTo(cur_x, cur_y);
            }
            ctx.closePath();
            ctx.stroke();
            ctx.restore();

            // Triangle 2 sliding copy
            ctx.save();
            ctx.strokeStyle = '#a200ff';
            ctx.lineWidth = 2.5;
            ctx.beginPath();
            for (let idx of [0, 2, 3]) {
                let i = idx;
                let cur_x = (pat_pts[i].x) * (1 - z) + (dest_center_2.x + raw_pts[i].x * scale_f) * z;
                let cur_y = (pat_pts[i].y) * (1 - z) + (dest_center_2.y + raw_pts[i].y * scale_f) * z;
                if (idx === 0) ctx.moveTo(cur_x, cur_y); else ctx.lineTo(cur_x, cur_y);
            }
            ctx.closePath();
            ctx.stroke();
            ctx.restore();
        }

        ctx.restore();
    });

    // 10 random selections of triangles in the point cloud (side lengths <= half point cloud box)
    let max_len = Math.min(right_w, right_h) * 0.5;
    let cloud_tri_sets = [];
    let attempt = 0;
    while (cloud_tri_sets.length < 10 && attempt < 1000) {
        attempt++;
        let i1 = Math.floor(seeded_random(3000 + attempt * 7) * 26) + 4;
        let i2 = Math.floor(seeded_random(3000 + attempt * 13) * 26) + 4;
        let i3 = Math.floor(seeded_random(3000 + attempt * 29) * 26) + 4;
        if (i1 === i2 || i2 === i3 || i1 === i3) continue;
        let p1 = cloud_pts[i1], p2 = cloud_pts[i2], p3 = cloud_pts[i3];
        let d12 = Math.hypot(p1.x - p2.x, p1.y - p2.y);
        let d23 = Math.hypot(p2.x - p3.x, p2.y - p3.y);
        let d31 = Math.hypot(p3.x - p1.x, p3.y - p1.y);
        if (d12 <= max_len && d23 <= max_len && d31 <= max_len) {
            cloud_tri_sets.push([i1, i2, i3]);
        }
    }

    vid.add_object('cloud_tri_search', { opacity: 0, step_idx: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let idx = Math.min(9, Math.max(0, Math.round(params.step_idx || 0)));
        if (idx < cloud_tri_sets.length) {
            let tris = cloud_tri_sets[idx];
            ctx.strokeStyle = '#dd6600';
            ctx.lineWidth = 2.5;
            ctx.beginPath();
            ctx.moveTo(cloud_pts[tris[0]].x, cloud_pts[tris[0]].y);
            ctx.lineTo(cloud_pts[tris[1]].x, cloud_pts[tris[1]].y);
            ctx.lineTo(cloud_pts[tris[2]].x, cloud_pts[tris[2]].y);
            ctx.closePath();
            ctx.stroke();
        }
        ctx.restore();
    });

    // Matched equilateral triangle in the point cloud (indices 0, 1, 2)
    vid.add_object('cloud_matched_equi', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#00a844';
        ctx.lineWidth = 3.5;
        ctx.beginPath();
        ctx.moveTo(cloud_pts[0].x, cloud_pts[0].y);
        ctx.lineTo(cloud_pts[1].x, cloud_pts[1].y);
        ctx.lineTo(cloud_pts[2].x, cloud_pts[2].y);
        ctx.closePath();
        ctx.stroke();
        ctx.restore();
    });

    // Final rigid body matching: red circles around all 4 points & full triangulation lines
    vid.add_object('cloud_final_match', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        // Full triangulation lines (purple for secondary triangle to complete structure)
        ctx.strokeStyle = '#a200ff';
        ctx.lineWidth = 3.5;
        ctx.beginPath();
        ctx.moveTo(cloud_pts[0].x, cloud_pts[0].y);
        ctx.lineTo(cloud_pts[2].x, cloud_pts[2].y);
        ctx.lineTo(cloud_pts[3].x, cloud_pts[3].y);
        ctx.closePath();
        ctx.stroke();

        // Red circles around all 4 matching points
        ctx.strokeStyle = '#dd1111';
        ctx.lineWidth = 3;
        for (let i = 0; i < 4; i++) {
            ctx.beginPath();
            ctx.arc(cloud_pts[i].x, cloud_pts[i].y, 14, 0, 2 * Math.PI);
            ctx.stroke();
        }
        ctx.restore();
    });

    // Find three distant points in the cloud (among random indices 4..29) for a large triangle
    let big_i1 = 4, big_i2 = 5, big_i3 = 6;
    let max_perim = 0;
    for (let i = 4; i < 20; i++) {
        for (let j = i + 1; j < 25; j++) {
            for (let k = j + 1; k < 30; k++) {
                let per = Math.hypot(cloud_pts[i].x - cloud_pts[j].x, cloud_pts[i].y - cloud_pts[j].y)
                        + Math.hypot(cloud_pts[j].x - cloud_pts[k].x, cloud_pts[j].y - cloud_pts[k].y)
                        + Math.hypot(cloud_pts[k].x - cloud_pts[i].x, cloud_pts[k].y - cloud_pts[i].y);
                if (per > max_perim) {
                    max_perim = per;
                    big_i1 = i; big_i2 = j; big_i3 = k;
                }
            }
        }
    }

    vid.add_object('cloud_big_triangle', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.strokeStyle = '#dd6600';
        ctx.lineWidth = 3.5;
        ctx.beginPath();
        ctx.moveTo(cloud_pts[big_i1].x, cloud_pts[big_i1].y);
        ctx.lineTo(cloud_pts[big_i2].x, cloud_pts[big_i2].y);
        ctx.lineTo(cloud_pts[big_i3].x, cloud_pts[big_i3].y);
        ctx.closePath();
        ctx.stroke();
        ctx.restore();
    });

    // Triangulation lines for the rigid body with movement animation support
    vid.add_object('cloud_rigid_lines', { opacity: 0, move_progress: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        let pts = [0, 1, 2, 3].map(i => transform_rb(cloud_pts[i], params.move_progress || 0));

        // Equilateral green triangle
        ctx.strokeStyle = '#00a844';
        ctx.lineWidth = 3.5;
        ctx.beginPath();
        ctx.moveTo(pts[0].x, pts[0].y);
        ctx.lineTo(pts[1].x, pts[1].y);
        ctx.lineTo(pts[2].x, pts[2].y);
        ctx.closePath();
        ctx.stroke();

        // Secondary purple triangle
        ctx.strokeStyle = '#a200ff';
        ctx.lineWidth = 3.5;
        ctx.beginPath();
        ctx.moveTo(pts[0].x, pts[0].y);
        ctx.lineTo(pts[2].x, pts[2].y);
        ctx.lineTo(pts[3].x, pts[3].y);
        ctx.closePath();
        ctx.stroke();
        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title and right point cloud box with points first
    vid.add_transition(['title', 'point_cloud'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause;

    // 2. After pause, fade in left Search Pattern square with its label and points
    vid.add_transition(['search_pattern'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause;

    // 3. Animate brute force searching for the 4 points (~10 times, holding 500ms on each)
    vid.add_transition(['bf_arrows'], t, 0.3, { opacity: 1, step_idx: 0 });
    t += 0.3 + 0.5;
    for (let m = 1; m < 10; m++) {
        vid.add_transition(['bf_arrows'], t, 0.05, { step_idx: m });
        t += 0.05 + 0.5;
    }

    // 4. Fade out all these arrows
    vid.add_transition(['bf_arrows'], t, 0.3, { opacity: 0 });
    t += 0.3 + pause;

    // 5. Draw in lines for the two pattern triangles
    vid.add_transition(['pat_triangles'], t, 0.5, { opacity: 1, slide_progress: 0 });
    t += 0.5 + pause;

    // 6. Animate these two triangles sliding down while the pattern box expands to meet the bottom border
    vid.add_transition(['pat_triangles', 'search_pattern'], t, 1.0, { slide_progress: 1 });
    t += 1.0 + pause;

    // 7. Animate 10 times randomly selecting triangles of points in the point cloud
    vid.add_transition(['cloud_tri_search'], t, 0.3, { opacity: 1, step_idx: 0 });
    t += 0.3 + 0.5;
    for (let m = 1; m < 10; m++) {
        vid.add_transition(['cloud_tri_search'], t, 0.05, { step_idx: m });
        t += 0.05 + 0.5;
    }
    vid.add_transition(['cloud_tri_search'], t, 0.3, { opacity: 0 });
    t += 0.3 + 0.2;

    // 8. Transition to matching the equilateral triangle in the point cloud
    vid.add_transition(['cloud_matched_equi'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 9. Fade in red circles around all 4 matching points & complete triangulation lines
    vid.add_transition(['cloud_final_match'], t, 0.6, { opacity: 1 });
    t += 0.6 + pause + 1.0;

    // 10. Fade out the rigid body markers in the point cloud
    vid.add_transition(['cloud_matched_equi', 'cloud_final_match'], t, 0.4, { opacity: 0 });
    t += 0.4 + pause;

    // 11. Fade in lines marking a really big triangle in the point cloud (distant points)
    vid.add_transition(['cloud_big_triangle'], t, 0.5, { opacity: 1 });
    t += 0.5 + pause;

    // 12. Fade out that large triangle
    vid.add_transition(['cloud_big_triangle'], t, 0.4, { opacity: 0 });
    t += 0.4 + pause;

    // 13. Fade in the triangulation lines at 50% opacity & turn the 4 matching dots red
    vid.add_transition(['cloud_rigid_lines'], t, 0.5, { opacity: 0.5, move_progress: 0 });
    vid.add_transition(['point_cloud'], t, 0.5, { highlight_red: 1, move_progress: 0 });
    t += 0.5 + pause;

    // 14. Animate the underlying 4 points sliding over by 20px with a small rotation around centroid
    vid.add_transition(['point_cloud'], t, 0.6, { move_progress: 1, highlight_red: 1 });
    t += 0.6 + pause;

    // 15. Then slide over and rotate the triangulation lines for the rigid body to match
    vid.add_transition(['cloud_rigid_lines'], t, 0.6, { move_progress: 1 });
    t += 0.6 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}

export function part13_kinematics(canvas) {
    let vid = new Timeline();
    vid.set_name('part13_kinematics');

    vid.add_object('title', { opacity: 0, text: 'Arm Tracking' }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        draw_title(ctx, params.text);
        ctx.restore();
    });

    let bg = slide_body_grid(canvas);
    let base_x = canvas.width * 0.42 - 40; // moved 40px left
    let base_y = bg._top + bg._height - 50;

    // Table
    vid.add_object('table', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        ctx.fillStyle = '#111111';
        ctx.fillRect(base_x - 300, base_y, 600, 24);
        ctx.restore();
    });

    let L1 = 210;
    let L2 = 170;
    let d1 = L1 - 35; // Marker #2 position on link 1
    let d2 = L2 - 35; // Marker #1 position on link 2

    // Target angles for the primary pose (in degrees)
    let target_a1 = 52;
    let target_a2 = -52;

    // Helper to compute world positions of joints and markers for given angles in degrees
    let get_kinematics = (a1_deg, a2_deg) => {
        let r1 = a1_deg * Math.PI / 180;
        let r2 = a2_deg * Math.PI / 180;
        let j1 = { x: base_x, y: base_y };
        let j2 = { x: base_x + L1 * Math.cos(r1), y: base_y - L1 * Math.sin(r1) };
        let wr = { x: j2.x + L2 * Math.cos(r1 + r2), y: j2.y - L2 * Math.sin(r1 + r2) };
        let m1 = { x: j2.x + d2 * Math.cos(r1 + r2), y: j2.y - d2 * Math.sin(r1 + r2) };
        let m2 = { x: base_x + d1 * Math.cos(r1), y: base_y - d1 * Math.sin(r1) };
        return { j1, j2, wr, m1, m2, r1, r2 };
    };

    // Calculate alternate pose ("Other Possibility") for exact same Marker #1 position
    let target_k = get_kinematics(target_a1, target_a2);
    let dx_m1 = target_k.m1.x - base_x;
    let dy_m1 = base_y - target_k.m1.y;
    let D = Math.hypot(dx_m1, dy_m1);
    let chord_angle = Math.atan2(dy_m1, dx_m1);
    let cos_val = Math.min(1, Math.max(-1, (L1 * L1 + D * D - d2 * d2) / (2 * L1 * D)));
    let inner_angle = Math.acos(cos_val);
    let alt_r1 = chord_angle - inner_angle;
    let alt_a1 = alt_r1 * 180 / Math.PI;
    let alt_a2 = -target_a2; // symmetrically bent in the opposite direction (+52 deg)

    // Drawing helper for robotic arm links and joints
    let draw_arm_visual = (ctx, a1, a2, link_color, show_green) => {
        let k = get_kinematics(a1, a2);

        // Draw Link 1 (pill shape)
        ctx.save();
        ctx.translate(k.j1.x, k.j1.y);
        ctx.rotate(-k.r1);
        ctx.fillStyle = link_color;
        ctx.strokeStyle = '#222222';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.arc(0, 0, 14, Math.PI/2, 3*Math.PI/2);
        ctx.lineTo(L1, -14);
        ctx.arc(L1, 0, 14, 3*Math.PI/2, 5*Math.PI/2);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
        ctx.restore();

        // Draw Link 2 (pill shape)
        ctx.save();
        ctx.translate(k.j2.x, k.j2.y);
        ctx.rotate(-(k.r1 + k.r2));
        ctx.fillStyle = link_color;
        ctx.strokeStyle = '#222222';
        ctx.lineWidth = 3;
        ctx.beginPath();
        ctx.arc(0, 0, 12, Math.PI/2, 3*Math.PI/2);
        ctx.lineTo(L2, -12);
        ctx.arc(L2, 0, 12, 3*Math.PI/2, 5*Math.PI/2);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();

        // Gripper at end of link 2
        ctx.translate(L2, 0);
        ctx.fillStyle = '#888888';
        ctx.fillRect(0, -15, 8, 30);
        ctx.strokeRect(0, -15, 8, 30);
        ctx.beginPath();
        ctx.moveTo(8, -15);
        ctx.lineTo(26, -15);
        ctx.lineTo(32, -6);
        ctx.moveTo(8, 15);
        ctx.lineTo(26, 15);
        ctx.lineTo(32, 6);
        ctx.lineWidth = 4.5;
        ctx.stroke();
        ctx.restore();

        // Joints
        for (let pt of [k.j1, k.j2]) {
            ctx.fillStyle = '#333333';
            ctx.beginPath();
            ctx.arc(pt.x, pt.y, 14, 0, 2 * Math.PI);
            ctx.fill();
            ctx.lineWidth = 2.5;
            ctx.strokeStyle = '#000000';
            ctx.stroke();
            ctx.fillStyle = '#ffffff';
            ctx.beginPath();
            ctx.arc(pt.x, pt.y, 4, 0, 2 * Math.PI);
            ctx.fill();
        }

        if (show_green > 0) {
            ctx.save();
            ctx.globalAlpha *= show_green;
            ctx.fillStyle = '#00a844';
            ctx.strokeStyle = '#000000';
            ctx.lineWidth = 2;
            for (let pt of [k.m1, k.m2]) {
                ctx.beginPath();
                ctx.arc(pt.x, pt.y, 10, 0, 2 * Math.PI);
                ctx.fill();
                ctx.stroke();
            }
            ctx.restore();
        }
        return k;
    };

    // Primary Robot Arm Object
    vid.add_object('robot_arm', { opacity: 0, a1: target_a1, a2: target_a2, show_angles: 0, unknown_angles: 0, show_green_markers: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        let k = draw_arm_visual(ctx, params.a1, params.a2, '#d0d8e8', params.show_green_markers || 0);

        // Draw angle markers and labels
        if (params.show_angles > 0) {
            ctx.save();
            ctx.globalAlpha *= params.show_angles;
            ctx.strokeStyle = '#0055dd';
            ctx.fillStyle = '#0055dd';
            ctx.lineWidth = 2.5;

            // Joint 1 angle arc (from horizontal table 0 to Link 1 boundary edge)
            ctx.beginPath();
            let ang_off1 = Math.asin(14 / 60);
            ctx.arc(k.j1.x, k.j1.y, 60, -k.r1 + ang_off1, 0, false);
            ctx.stroke();

            ctx.font = 'bold 20px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            let label1 = params.unknown_angles > 0 ? '?°' : Math.round(params.a1) + '°';
            let mid1 = -k.r1 / 2;
            ctx.fillText(label1, k.j1.x + 85 * Math.cos(mid1), k.j1.y + 85 * Math.sin(mid1));

            // Joint 2 interior angle arc
            ctx.strokeStyle = '#a200ff';
            ctx.fillStyle = '#a200ff';
            let a_back = -k.r1 + Math.PI; // direction back towards joint 1
            let a_fwd = -(k.r1 + k.r2);  // direction forward towards wrist
            let delta = a_fwd - a_back;
            while (delta > Math.PI) delta -= 2 * Math.PI;
            while (delta < -Math.PI) delta += 2 * Math.PI;
            let dir = Math.sign(delta) || 1;

            let start_ang2 = a_back + dir * Math.asin(14 / 55);
            let end_ang2 = a_fwd - dir * Math.asin(12 / 55);
            ctx.beginPath();
            ctx.arc(k.j2.x, k.j2.y, 55, start_ang2, end_ang2, dir < 0);
            ctx.stroke();

            let val2 = Math.round(180 - Math.abs(params.a2));
            let label2 = params.unknown_angles > 0 ? '?°' : val2 + '°';
            let mid2 = a_back + delta / 2;
            ctx.fillText(label2, k.j2.x + 80 * Math.cos(mid2), k.j2.y + 80 * Math.sin(mid2));
            ctx.restore();
        }

        ctx.restore();
    });

    // Alternate Robot Arm ("Other Possibility")
    vid.add_object('alt_robot_arm', { opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;
        let k = draw_arm_visual(ctx, alt_a1, alt_a2, '#f0d0c0', 0);

        // Multiline label on right side of alternate orientation
        ctx.fillStyle = '#dd6600';
        ctx.font = 'bold 24px sans-serif';
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillText('Other', k.j2.x + 30, k.j2.y - 12);
        ctx.fillText('Possibility', k.j2.x + 30, k.j2.y + 16);
        ctx.restore();
    });

    // Fixed Red Markers and Labels
    vid.add_object('red_markers', { opacity: 0, show_m1: 0, show_m2: 0, m1_label_opacity: 0, m2_label_opacity: 0, triangulated_label_opacity: 0 }, (ctx, params) => {
        if (params.opacity <= 0 && params.show_m1 <= 0 && params.show_m2 <= 0) return;
        ctx.save();
        ctx.globalAlpha *= Math.max(params.opacity, params.show_m1, params.show_m2);

        let k_fixed = target_k; // target_k contains true marker world coordinates
        ctx.fillStyle = '#dd1111';
        ctx.strokeStyle = '#000000';
        ctx.lineWidth = 2;

        if (params.show_m1 > 0) {
            ctx.beginPath();
            ctx.arc(k_fixed.m1.x, k_fixed.m1.y, 10, 0, 2 * Math.PI);
            ctx.fill();
            ctx.stroke();

            if (params.m1_label_opacity > 0) {
                ctx.save();
                ctx.globalAlpha *= params.m1_label_opacity;
                ctx.fillStyle = '#dd1111';
                ctx.strokeStyle = '#dd1111';
                ctx.font = 'bold 22px sans-serif';
                ctx.textAlign = 'right';
                let lbl1_x = k_fixed.m1.x - 45;
                let lbl1_y = k_fixed.m1.y - 40;
                ctx.fillText('Marker', lbl1_x, lbl1_y);
                drawArrowPos(ctx, { x: lbl1_x + 10, y: lbl1_y + 6 }, { x: k_fixed.m1.x - 7, y: k_fixed.m1.y - 7 }, 2.5, 12, false);
                ctx.restore();
            }
        }

        if (params.show_m2 > 0) {
            ctx.beginPath();
            ctx.arc(k_fixed.m2.x, k_fixed.m2.y, 10, 0, 2 * Math.PI);
            ctx.fill();
            ctx.stroke();

            if (params.m2_label_opacity > 0) {
                ctx.save();
                ctx.globalAlpha *= params.m2_label_opacity;
                ctx.fillStyle = '#dd1111';
                ctx.strokeStyle = '#dd1111';
                ctx.font = 'bold 22px sans-serif';
                ctx.textAlign = 'right';
                ctx.fillText('Marker #2', k_fixed.m2.x - 75, k_fixed.m2.y + 6);
                drawArrowPos(ctx, { x: k_fixed.m2.x - 70, y: k_fixed.m2.y }, { x: k_fixed.m2.x - 15, y: k_fixed.m2.y }, 2.5, 12, false);
                ctx.restore();
            }
        }

        if (params.triangulated_label_opacity > 0) {
            ctx.save();
            ctx.globalAlpha *= params.triangulated_label_opacity;
            let label_x = k_fixed.m1.x + 140;
            let label_y = bg._top + 60;
            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 26px sans-serif';
            ctx.textAlign = 'center';
            ctx.fillText('Triangulated Points', label_x, label_y);

            ctx.strokeStyle = '#dd1111';
            ctx.fillStyle = '#dd1111';
            drawArrowPos(ctx, { x: label_x - 40, y: label_y + 15 }, { x: k_fixed.m1.x + 10, y: k_fixed.m1.y - 6 }, 2.5, 14, false);
            drawArrowPos(ctx, { x: label_x - 110, y: label_y + 15 }, { x: k_fixed.m2.x + 10, y: k_fixed.m2.y - 6 }, 2.5, 14, false);
            ctx.restore();
        }

        ctx.restore();
    });

    // Optimization text & Current Guess label
    vid.add_object('optimization_labels', { opacity: 0, show_guess_text: 0, show_error_text: 0, current_a1: 90, current_a2: 0 }, (ctx, params) => {
        if (params.opacity <= 0) return;
        ctx.save();
        ctx.globalAlpha *= params.opacity;

        let k_cur = get_kinematics(params.current_a1, params.current_a2);
        let k_tgt = target_k;

        if (params.show_guess_text > 0) {
            ctx.save();
            ctx.globalAlpha *= params.show_guess_text;
            ctx.fillStyle = '#00a844';
            ctx.strokeStyle = '#00a844';
            ctx.font = 'bold 26px sans-serif';
            ctx.textAlign = 'right';
            ctx.textBaseline = 'middle';
            let label_end_x = k_cur.j2.x - 70;
            let label_y = k_cur.j2.y - 45;
            ctx.fillText('Current Guess', label_end_x, label_y);

            drawArrowPos(ctx, { x: label_end_x + 8, y: label_y + 12 }, { x: k_cur.m2.x - 12, y: k_cur.m2.y - 5 }, 2.5, 14, false);
            drawArrowPos(ctx, { x: label_end_x + 8, y: label_y - 12 }, { x: k_cur.m1.x - 12, y: k_cur.m1.y - 2 }, 2.5, 14, false);
            ctx.restore();
        }

        if (params.show_error_text > 0) {
            let e1 = Math.hypot(k_cur.m1.x - k_tgt.m1.x, k_cur.m1.y - k_tgt.m1.y);
            let e2 = Math.hypot(k_cur.m2.x - k_tgt.m2.x, k_cur.m2.y - k_tgt.m2.y);
            let total_err = Math.round(e1 * e1 + e2 * e2);

            let err_x = canvas.width - 200;
            let err_y = bg._top + 100;
            ctx.fillStyle = '#ffffff';
            ctx.strokeStyle = '#000000';
            ctx.lineWidth = 2;
            ctx.fillRect(err_x - 130, err_y - 30, 260, 60);
            ctx.strokeRect(err_x - 130, err_y - 30, 260, 60);

            ctx.fillStyle = '#dd1111';
            ctx.font = 'bold 28px sans-serif';
            ctx.textAlign = 'center';
            ctx.textBaseline = 'middle';
            ctx.fillText('Error = ' + total_err, err_x, err_y);
        }

        ctx.restore();
    });

    let pause = 0.5;
    let t = 0;

    // 1. Fade in title, table, and initial robotic arm
    vid.add_transition(['title', 'table'], t, 0.6, { opacity: 1 });
    vid.add_transition(['robot_arm'], t, 0.6, { opacity: 1, a1: target_a1, a2: target_a2, show_angles: 0 });
    t += 0.6 + pause;

    // 2. Fade in angle markers for each joint displaying current angle in degrees
    vid.add_transition(['robot_arm'], t, 0.5, { show_angles: 1, unknown_angles: 0 });
    t += 0.5 + pause;

    // 3. Animate moving first joint back and forth and use ? markers (slowed down by 50%)
    vid.add_transition(['robot_arm'], t, 0.8, { a1: 75, unknown_angles: 1 });
    t += 0.8;
    vid.add_transition(['robot_arm'], t, 1.2, { a1: 30 });
    t += 1.2;
    vid.add_transition(['robot_arm'], t, 0.8, { a1: target_a1 });
    t += 0.8 + 0.4;

    // 4. Animate moving second joint back and forth (showing ?°) (slowed down by 50%)
    vid.add_transition(['robot_arm'], t, 0.8, { a2: -20 });
    t += 0.8;
    vid.add_transition(['robot_arm'], t, 1.2, { a2: -80 });
    t += 1.2;
    vid.add_transition(['robot_arm'], t, 0.8, { a2: target_a2 });
    t += 0.8 + pause;

    // 5. Fade out angle markers and labels
    vid.add_transition(['robot_arm'], t, 0.4, { show_angles: 0 });
    t += 0.4 + pause;

    // 6. Place one marker (red dot slightly behind wrist) with text label "Marker"
    vid.add_transition(['red_markers'], t, 0.5, { opacity: 1, show_m1: 1, m1_label_opacity: 1 });
    t += 0.5 + pause;

    // 7. Fade in second possible orientation at 100% opacity ("Other Possibility"), fade original arm to 40%
    vid.add_transition(['alt_robot_arm'], t, 0.6, { opacity: 1 });
    vid.add_transition(['robot_arm'], t, 0.6, { opacity: 0.4 });
    t += 0.6 + pause + 1.0;

    // 8. Reverse fade and hide all text (back to just seeing original arm + marker 1)
    vid.add_transition(['alt_robot_arm'], t, 0.5, { opacity: 0 });
    vid.add_transition(['robot_arm'], t, 0.5, { opacity: 1 });
    vid.add_transition(['red_markers'], t, 0.5, { m1_label_opacity: 0 });
    t += 0.5 + pause;

    // 9. Fade in second marker behind joint 2 with text "Marker #2"
    vid.add_transition(['red_markers'], t, 0.5, { show_m2: 1, m2_label_opacity: 1 });
    t += 0.5 + pause;

    // 10. Quickly animate arm to vertical (a1=90, a2=0), keep marker positions fixed, fade in real numbers for angles
    vid.add_transition(['red_markers'], t, 0.4, { m2_label_opacity: 0 });
    vid.add_transition(['robot_arm'], t, 0.6, { a1: 90, a2: 0, show_angles: 1, unknown_angles: 0 });
    t += 0.6 + pause;

    // 11. Fade in text pointing to the two markers that says "Triangulated Points"
    vid.add_transition(['red_markers'], t, 0.5, { triangulated_label_opacity: 1 });
    t += 0.5 + pause;

    // 12. Fade in green markers on the arm, switch label to "Current Guess" & show error
    vid.add_transition(['robot_arm'], t, 0.5, { show_green_markers: 1 });
    vid.add_transition(['red_markers'], t, 0.5, { triangulated_label_opacity: 0 });
    vid.add_transition(['optimization_labels'], t, 0.5, { opacity: 1, show_guess_text: 1, show_error_text: 1, current_a1: 90, current_a2: 0 });
    t += 0.5 + pause + 0.5;

    // 13. Animate slowly applying gradient descent to joint angles until SSE is minimized (Error = 0), fading out Current Guess
    vid.add_transition(['robot_arm'], t, 3.5, { a1: target_a1, a2: target_a2 });
    vid.add_transition(['optimization_labels'], t, 0.5, { show_guess_text: 0 });
    vid.add_transition(['optimization_labels'], t, 3.5, { current_a1: target_a1, current_a2: target_a2 });
    t += 3.5 + pause + 2.0;

    vid.set_duration(t);
    return vid;
}





