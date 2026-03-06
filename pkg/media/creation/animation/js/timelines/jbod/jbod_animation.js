import { Timeline, draw_title, deg2rad, draw_box, slide_body_grid, DiagramBox, WireBundle, Wire, shallow_copy, draw_multiline_text, draw_box_text } from '../../utils.js';
import { hexToRgba } from '../../hex_to_rgba.js';
import { drawArrow } from '../../arrow.js';
import { getPointAtY } from '../../y_point.js';
import { drawPolyline, drawSequentialChains, drawShearedSquare } from '../../sheared_square.js';
import { math_to_img, math_scale } from '../../mathjax.js';
import { drawCenteredTable } from '../../centered_table.js';
import { draw_graph } from '../3d_printer/motion_animation.js';
import { getInterpolatedY, interpolateValue } from '../../linear_interp.js';
import { getObjectAlpha } from '../../staggered_fade.js';

export async function configure(canvas) {
    // return part2_nas_video(canvas);
    // return part3_video(canvas);
    // return part4_g90_video(canvas);
    // return part5_power_distribution_video(canvas);
    // return part15_flow_video(canvas);
    return part15_raid_video(canvas);
    // return part15_topology_video(canvas);
}

function part15_topology_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part15_topology");

    vid.add_object('title', { opacity: 0, text: 'ZFS Disk Topology' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);

    let disks = [];

    for (var i = 0; i < 3; i++) {
        for (var j = 0; j < 15; j++) {

            let pos = body_grid.center();

            pos.y += (i - 1) * 120;
            pos.x += (j - 7) * 60;

            disks.push(new DiagramBox({
                text: '',
                font_size: 18,
                width: 40,
                height: 100,
                position: pos
            }))

        }
    }

    vid.add_object('disks', { opacity: 1, t: 0 }, (ctx, params) => {
        disks.map((box, i) => {
            ctx.save();

            ctx.globalAlpha *= getObjectAlpha(i, disks.length, params.t, 0.5);

            box.draw(ctx);

            ctx.restore();
        });
    });

    let highlights = [];

    function make_box(i, j) {
        let x_min = Math.min(disks[i].left_center().x, disks[j].left_center().x);
        let x_max = Math.max(disks[i].right_center().x, disks[j].right_center().x);

        let y_min = Math.min(disks[i].top_center().y, disks[j].top_center().y);
        let y_max = Math.max(disks[i].bottom_center().y, disks[j].bottom_center().y);

        let pos = { x: (x_max + x_min) / 2, y: (y_min + y_max) / 2 };
        let pad = 5;

        highlights.push(new DiagramBox({
            text: 'Sub Pool\n#' + (highlights.length + 1),
            font_size: 22,
            text_color: 'rgba(235, 231, 231, 1)',
            background_color: hexToRgba('#000', 0.4),
            width: (x_max - x_min) + 2 * pad,
            height: (y_max - y_min) + 2 * pad,
            position: pos
        }));
    }

    function index_of(i, j) {
        return i * 15 + j;
    }

    make_box(index_of(0, 0), index_of(0, 5));
    make_box(index_of(0, 6), index_of(0, 6 + 6));
    make_box(index_of(1, 0), index_of(1, 5));
    make_box(index_of(1, 6), index_of(1, 6 + 6));
    make_box(index_of(2, 0), index_of(2, 5));
    make_box(index_of(2, 6), index_of(2, 6 + 6));
    make_box(index_of(0, 13), index_of(2, 14));


    vid.add_object('highlight', { opacity: 1, t: 0 }, (ctx, params) => {
        ctx.setLineDash([5, 5]);
        highlights.map((box, i) => {

            ctx.save();

            ctx.globalAlpha *= getObjectAlpha(i, highlights.length, params.t, 0.9);

            box.draw(ctx);

            ctx.restore();
        });
    });


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['disks'], t, 1, { t: 1 });
    t += 1;
    t += pause;

    vid.add_transition(['highlight'], t, 2, { t: 1 });
    t += 2;
    t += pause;


    vid.set_duration(t);


    return vid;
}

function part15_raid_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part15_raid");

    vid.add_object('title', { opacity: 0, text: 'ZFS RAID' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 3);

    let text = 'HELLO_WORLD!';

    let text_size = 30;

    let char_width;
    // {
    //     let ctx = canvas.getContext('2d');
    //     ctx.font = `${text_size}px "Noto Sans Mono"`;
    //     char_width = ctx.measureText(" ").width;
    // }
    // console.log(char_width);
    char_width = 17.999984741210938;

    let text_center = box_grid.cell(0.2, 1).center();

    let text_chunks = [
        'HEL', 'LO_', 'WOR', 'LD!'
    ];

    let text_boxes = [];

    for (var i = 0; i < text_chunks.length; i++) {

        let pos = shallow_copy(text_center);
        pos.x += (i - 1.5) * text_chunks[0].length * char_width;

        text_boxes.push(new DiagramBox({
            text: text_chunks[i],
            font_size: text_size,
            font_family: "Noto Sans Mono",
            background_color: hexToRgba('#aaccee', 0),
            stroke_color: hexToRgba('#000', 0),
            width: (1 + text_chunks[0].length) * char_width,
            height: (1 + text_chunks[0].length) * char_width,
            position: pos
        }))
    }


    let parity_boxes = [];
    let parity_colors = ['#f00', '#084'];
    parity_colors.map((color) => {
        parity_boxes.push(new DiagramBox({
            text: '',
            font_size: text_size,
            font_family: "Noto Sans Mono",
            background_color: color,
            // stroke_color: hexToRgba('#000', 0),
            width: (1 + text_chunks[0].length) * char_width,
            height: (1 + text_chunks[0].length) * char_width,
            position: { x: 0, y: 0 }
        }));
    })

    let disks = [];
    for (var i = 0; i < text_chunks.length + parity_boxes.length; i++) {
        disks.push(new DiagramBox({
            text: 'Disk',
            font_size: 20,
            font_family: "Noto Sans Mono",
            // stroke_color: hexToRgba('#000', 0),
            width: (3 + text_chunks[0].length) * char_width,
            height: (6 + text_chunks[0].length) * char_width,
            position: { x: 0, y: 0 },
            text_offset: { x: 0, y: -65 }
        }))
    }

    let failures = [];

    vid.add_object('failures', { opacity: 1, indexes: [] }, (ctx, params) => {
        failures = params.indexes;

        disks.map((disk, i) => {
            let alpha = 1;

            if (params.indexes.includes(i)) {
                alpha = 0.1;
            }

            disk._background_color = hexToRgba('#aaccee', alpha);
            if (i < text_boxes.length) {
                text_boxes[i]._background_color = hexToRgba('#aaccee', alpha);
            } else {
                parity_boxes[i - text_boxes.length]._background_color = hexToRgba(parity_colors[i - text_boxes.length], alpha);
            }

        });

    })

    vid.add_object('disks', { opacity: 0 }, (ctx, params) => {
        disks.map((box, i) => {
            let pos;
            if (i < text_chunks.length) {
                pos = text_boxes[i]._position;
            } else {
                pos = parity_boxes[i - text_chunks.length]._position;
            }
            pos = shallow_copy(pos);
            // pos.y += 20;

            box._position = pos;

            box.draw(ctx);
        });
    });


    vid.add_object('text_chunks', { opacity: 0, t: 0, p: 0 }, (ctx, params) => {
        let offset = 1.5 + 1 * params.p;

        text_boxes.map((box, i) => {
            let alpha = params.t;
            if (failures.includes(i)) {
                alpha = 0.1;
            }

            box._background_color = hexToRgba('#aaccee', alpha);
            box._stroke_color = hexToRgba('#000', params.t);

            let pos = shallow_copy(text_center);
            pos.x += (i - offset) * (text_chunks[0].length * char_width + params.t * 60);
            box._position = pos;

            box.draw(ctx);
        })
    });



    vid.add_object('parity', { opacity: 0 }, (ctx, params) => {
        let offset = 2.5;

        parity_boxes.map((box, i) => {
            let j = i + text_chunks.length;

            // Same as the text_chunks equation.
            let pos = shallow_copy(text_center);
            pos.x += (j - offset) * (text_chunks[0].length * char_width + 1 * 60);
            box._position = pos;

            box.draw(ctx);
        });
    });




    let read_pos = shallow_copy(text_center);
    read_pos.y += 250;

    let read = new DiagramBox({
        text,
        font_size: text_size,
        font_family: "Noto Sans Mono",
        background_color: hexToRgba('#000', 0),
        stroke_color: hexToRgba('#000', 0.1),
        width: (2 + text_chunks[0].length * 4) * char_width,
        height: 2.5 * char_width,
        position: read_pos
    });


    vid.add_object('read', { opacity: 0 }, (ctx, params) => {
        read.draw(ctx);
    });

    vid.add_object('read_arrows', { opacity: 1, t: 0, disks: [0, 1, 2, 3] }, (ctx, params) => {

        disks.map((disk, i) => {

            if (!params.disks.includes(i)) {
                return;
            }

            let a = disk.bottom_center();
            let b = read.top_center();
            b.x += (i - 2.5) * 30;

            let lines = [[
                a, b
            ]]

            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            drawSequentialChains(ctx, lines, params.t);
            ctx.restore();
        })

    });


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['text_chunks'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;



    vid.add_transition(['text_chunks'], t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['text_chunks'], t, 0.5, { p: 1 });
    vid.add_transition(['parity'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['disks'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['read_arrows'], t, 0.5, { t: 1 });
    t += 0.5;
    vid.add_transition(['read'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_key_frame('failures', t, { indexes: [2, 4] });
    vid.add_key_frame('read_arrows', t, { disks: [0, 1, 3, 5] });
    t += 0.5;


    vid.set_duration(t);

    return vid;
}

function part15_flow_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part15_flow");

    vid.add_object('title', { opacity: 0, text: 'Data Flow' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });


    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 3);

    let disk_boxes = [];

    for (var i = -3.5; i <= 3.5; i++) {
        let pos = box_grid.cell(1, 2).center();

        disk_boxes.push(new DiagramBox({
            text: 'Disk',
            font_size: 18,
            width: 100,
            height: 40,
            position: { x: pos.x + 40, y: pos.y + i * 50 }
        }));

    }

    vid.add_object('disks', { opacity: 0 }, (ctx, params) => {
        disk_boxes.map((box) => {
            box.draw(ctx);
        });
    });


    let expanders = [];
    [0.25, 1.75].map((row) => {
        let pos = box_grid.cell(row, 1).center();

        expanders.push(new DiagramBox({
            text: 'SAS Expander',
            font_size: 18,
            width: 150,
            height: 190,
            position: pos
        }));
    })

    vid.add_object('expanders', { opacity: 0 }, (ctx, params) => {
        expanders.map((box) => {
            box.draw(ctx);
        });
    });

    vid.add_object('disk_wires', { opacity: 1, t: 0 }, (ctx, params) => {
        for (var i = 0; i < disk_boxes.length; i++) {
            let a = disk_boxes[i].left_center();
            let b = expanders[Math.floor(i / 4)].right_center();
            b.y = a.y;

            let lines = [[
                a, b
            ]]

            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            drawSequentialChains(ctx, lines, params.t);
            ctx.restore();
        }
    });


    let hba_pos = box_grid.cell(0, 0).center();
    let hba = new DiagramBox({
        text: 'SAS\nHBA',
        font_size: 18,
        width: 180,
        height: 100,
        position: hba_pos
    });

    vid.add_object('hba', { opacity: 0 }, (ctx, params) => {
        hba.draw(ctx);
    });

    vid.add_object('expander_wires', { opacity: 1, t: 0 }, (ctx, params) => {
        {
            let a = expanders[0].left_center();
            a.y = hba.right_center().y - 20;

            let b = hba.right_center();
            b.y = a.y;

            let lines = [[
                a, b
            ]]

            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 8;
            drawSequentialChains(ctx, lines, params.t);
            ctx.restore();
        }

        {
            let a = expanders[1].left_center();

            let b = shallow_copy(a);
            b.x -= 30;

            let c = shallow_copy(b);
            c.y = hba.right_center().y + 20;

            let d = shallow_copy(c);
            d.x = hba.right_center().x;

            let lines = [[
                a, b, c, d
            ]]

            ctx.save();
            ctx.strokeStyle = '#000';
            ctx.lineWidth = 8;
            drawSequentialChains(ctx, lines, params.t);
            ctx.restore();
        }
    });

    let cpu_pos = box_grid.cell(1.5, 0).center();

    let cpu_box = new DiagramBox({
        text: 'NAS\nCPU',
        font_size: 18,
        width: 180,
        height: 100,
        position: cpu_pos
    })

    vid.add_object('cpu', { opacity: 0 }, (ctx, params) => {
        cpu_box.draw(ctx);
    });

    vid.add_object('pcie', { opacity: 1, t: 0 }, (ctx, params) => {
        let a = hba.bottom_center();
        let b = cpu_box.top_center();

        let lines = [[
            a, b
        ]]

        ctx.save();
        ctx.strokeStyle = '#000';
        ctx.lineWidth = 10;
        drawSequentialChains(ctx, lines, params.t);
        ctx.restore();
    });



    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['expanders', 'disks'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['disk_wires'], t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['hba'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['expander_wires'], t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['cpu'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['pcie'], t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.set_duration(t);

    return vid;
}


function part5_power_distribution_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part5_power_distribution");

    vid.add_object('title', { opacity: 0, text: 'PSU Power Distribution' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let disk_box = new DiagramBox({
        text: 'Disk',
        font_size: 18,
        width: 100,
        height: 40,
        position: { x: 0, y: 0 }
    });

    let left_psu_pos = {
        x: canvas.width / 2 - 150,
        y: canvas.height / 2 - 100
    };
    let right_psu_pos = {
        x: canvas.width / 2 + 150,
        y: left_psu_pos.y
    };



    let left_psu_box = new DiagramBox({
        text: 'Left\nPSU',
        width: 200,
        height: 100,
        position: left_psu_pos
    });
    let right_psu_box = new DiagramBox({
        text: 'Right\nPSU',
        width: 200,
        height: 100,
        position: right_psu_pos
    });

    vid.add_object('psus', { opacity: 0 }, (ctx, params) => {
        left_psu_box.draw(ctx);
        right_psu_box.draw(ctx);
    })

    vid.add_object('disks', { opacity: 0, t: 0 }, (ctx, params) => {

        for (var i = 0; i < 15; i++) {
            ctx.save();

            let pos = {
                x: canvas.width / 2,
                y: canvas.height / 2 + 150
            };

            let x = (i - 7) * 50;
            pos.x += x;

            ctx.translate(pos.x, pos.y);
            ctx.rotate(deg2rad(-90));

            disk_box.draw(ctx);

            ctx.restore();

            let psu_pos;
            let color;
            let rel_i;

            if (i < 7) {
                psu_pos = left_psu_box.bottom_center();
                rel_i = i;
                color = '#f00';
            } else {
                psu_pos = right_psu_box.bottom_center();
                rel_i = i - 7;
                color = '#0b4';
            }

            let lines = [[
                { x: pos.x, y: pos.y - (disk_box._width / 2) },
                { x: psu_pos.x + (rel_i - 3) * 5, y: psu_pos.y },
            ]]

            ctx.save();
            ctx.strokeStyle = color;
            ctx.lineWidth = 4;
            drawSequentialChains(ctx, lines, params.t);
            ctx.restore();
        }
    });



    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'disks', 'psus'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['disks'], t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.set_duration(t);

    return vid;

}

function part4_g90_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part4_g90");

    vid.add_object('title', { opacity: 0, text: 'G90 Steel (Side View)' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);

    let steel_pos = body_grid.center();

    let steel_box = new DiagramBox({
        text: 'Steel',
        width: 600,
        height: 80,
        position: steel_pos,
        background_color: '#ccc',
        font_size: 20,
        text_offset: { x: -350, y: 0 },
    });


    let zinc_box = new DiagramBox({
        text: 'Zinc',
        width: 600,
        height: 20,
        position: steel_pos,
        background_color: '#eee',
        font_size: 20,
        text_offset: { x: -350, y: 0 },
    });

    vid.add_object('steel', { opacity: 0 }, (ctx, params) => {
        steel_box.draw(ctx);

        ctx.save();
        ctx.translate(0, -(zinc_box._height + steel_box._height) / 2);
        zinc_box.draw(ctx);
        ctx.restore();

        ctx.save();
        ctx.translate(0, (zinc_box._height + steel_box._height) / 2);
        zinc_box.draw(ctx);
        ctx.restore();
    });

    vid.add_object('hole', { opacity: 1, t: 0 }, (ctx, params) => {
        ctx.beginPath();

        ctx.moveTo(canvas.width / 2, 0);
        ctx.lineTo(canvas.width / 2, canvas.height * params.t);;

        ctx.lineWidth = 40;
        ctx.strokeStyle = 'white';
        ctx.stroke();
    })

    vid.add_object('laser', { opacity: 1, t: 0 }, (ctx, params) => {

        ctx.beginPath();

        ctx.moveTo(canvas.width / 2, 0);
        ctx.lineTo(canvas.width / 2, canvas.height * params.t);;

        ctx.lineWidth = 40;
        ctx.strokeStyle = 'red';
        ctx.stroke();
    });


    vid.add_object('rust', { opacity: 0 }, (ctx, params) => {

        let top = steel_box.top_center().y;
        let bottom = steel_box.bottom_center().y;

        let left = canvas.width / 2 - 20;
        let right = canvas.width / 2 + 20;

        ctx.beginPath();

        ctx.moveTo(left, top);
        ctx.lineTo(left, bottom);
        ctx.moveTo(right, top);
        ctx.lineTo(right, bottom);

        ctx.lineWidth = 4;
        ctx.strokeStyle = '#8B3103';
        ctx.stroke();

        ctx.translate(left + 15, bottom + 50);
        draw_multiline_text(ctx, {
            text: `^ Rust`,
            font_size: 30,
            text_align: 'left',
            color: '#8B3103'
        });
    })

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'steel'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['laser', 'hole'], t, 0.5, { t: 1 });
    t += 0.5;

    vid.add_transition(['laser'], t, 0.5, { opacity: 0 });
    t += 0.5;
    t += pause;


    vid.add_transition(['rust'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.set_duration(t);

    return vid;
}

function part3_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part3");

    vid.add_object('title', { opacity: 0, text: 'The Plan' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(1, 2);

    let nas_pos = box_grid.cell(0, 0).center();

    let nas_box = new DiagramBox({
        text: 'Existing NAS',
        width: 300,
        height: 400,
        position: nas_pos,
        font_size: 20,
        text_offset: {
            x: 0,
            y: -180,
        }
    });

    let disk_pos = shallow_copy(nas_pos);
    disk_pos.y += 100;

    let disk_box = new DiagramBox({
        text: 'Disk',
        font_size: 18,
        width: 100,
        height: 40,
        position: { x: 0, y: 0 }
    });



    let back_fans_pos = shallow_copy(nas_pos);
    back_fans_pos.y += 20;

    let back_fans_box = new DiagramBox({
        text: 'Back Fans',
        width: 250,
        height: 30,
        font_size: 18,
        position: back_fans_pos
    });

    let front_fans_pos = shallow_copy(nas_pos);
    front_fans_pos.y += 180;

    let front_fans_box = new DiagramBox({
        text: 'Front Fans',
        width: 250,
        height: 30,
        font_size: 18,
        position: front_fans_pos
    });

    let psu_pos = shallow_copy(nas_pos);
    psu_pos.y -= 90;
    psu_pos.x -= 85;

    let psu_box = new DiagramBox({
        text: 'PSU',
        width: 80,
        height: 120,
        font_size: 18,
        position: psu_pos
    });

    let cpu_pos = shallow_copy(nas_pos);
    cpu_pos.y -= 80;

    let cpu_box = new DiagramBox({
        text: 'CPU /\nMother\nBoard',
        width: 80,
        height: 140,
        font_size: 18,
        position: cpu_pos
    });

    let sas_hba_pos = shallow_copy(nas_pos);
    sas_hba_pos.y -= 110;
    sas_hba_pos.x += 85;

    let sas_hba_box = new DiagramBox({
        text: 'SAS\nHBA',
        width: 80,
        height: 80,
        font_size: 18,
        position: sas_hba_pos,
        // background_color: '#fff',
    });



    let jbod_pos = box_grid.cell(0, 1).center();

    let jbod_box = new DiagramBox({
        text: 'JBOD (new)',
        width: 300,
        height: 400,
        position: jbod_pos,
        font_size: 20,
        text_offset: {
            x: 0,
            y: -180,
        }
    });

    let sas_expander_pos = shallow_copy(jbod_pos);
    sas_expander_pos.y -= 140;
    sas_expander_pos.x -= 80;

    let sas_expander_box = new DiagramBox({
        text: 'SAS\nExpander',
        width: 90,
        height: 50,
        font_size: 16,
        position: sas_expander_pos
    });

    let main_board_pos = shallow_copy(jbod_pos);
    main_board_pos.y -= 140;
    main_board_pos.x += 20;

    let main_board_box = new DiagramBox({
        text: 'Mini\nBoard',
        width: 90,
        height: 50,
        font_size: 16,
        position: main_board_pos
    });

    let new_psu_pos = shallow_copy(jbod_pos);
    new_psu_pos.y -= 140;
    new_psu_pos.x += 100;

    let new_psu_box = new DiagramBox({
        text: 'PSU',
        width: 50,
        height: 50,
        font_size: 16,
        position: new_psu_pos
    });


    vid.add_object('nas', { opacity: 0 }, (ctx, params) => {
        nas_box.draw(ctx);
    });

    vid.add_object('nas_internals', { opacity: 1, t: 0 }, (ctx, params) => {

        let objects = [];

        objects.push(front_fans_box);

        let disk_indexes = [];
        for (var i = 0; i < 5; i++) {
            disk_indexes.push(i);
        }

        disk_indexes.map((i) => {
            objects.push({
                draw: (ctx) => {
                    ctx.save();

                    let x = (i - 2) * 50;

                    ctx.translate(disk_pos.x + x, disk_pos.y);
                    ctx.rotate(deg2rad(-90));

                    disk_box.draw(ctx);

                    ctx.restore();
                }
            });
        })


        objects.push(back_fans_box);
        objects.push(psu_box);
        objects.push(cpu_box);

        objects.map((obj, i) => {
            ctx.save();

            ctx.globalAlpha *= getObjectAlpha(i, objects.length, params.t);
            obj.draw(ctx);

            ctx.restore();
        })

    })

    vid.add_object('jbod', { opacity: 0 }, (ctx, params) => {
        jbod_box.draw(ctx);
    });


    vid.add_object('sas_cards', { opacity: 0 }, (ctx, params) => {
        sas_hba_box.draw(ctx);
        sas_expander_box.draw(ctx);
    });


    vid.add_object('jbod_psu', { opacity: 0 }, (ctx, params) => {
        new_psu_box.draw(ctx);
    });

    vid.add_object('jbod_fans', { opacity: 0 }, (ctx, params) => {
        ctx.save();
        ctx.translate(jbod_pos.x - nas_pos.x, 0);
        front_fans_box.draw(ctx);
        ctx.restore();

        ctx.save();
        ctx.translate(jbod_pos.x - nas_pos.x, -110);
        back_fans_box.draw(ctx);
        ctx.restore();
    });

    vid.add_object('jbod_main', { opacity: 0 }, (ctx, params) => {
        main_board_box.draw(ctx);
    });


    vid.add_object('jbod_disks', { opacity: 1, t: 0 }, (ctx, params) => {

        let objects = [];

        let disk_indexes = [];
        for (var i = 0; i < 10; i++) {
            disk_indexes.push(i);
        }

        disk_indexes.map((i) => {
            objects.push({
                draw: (ctx) => {
                    ctx.save();

                    let x = ((i % 5) - 2) * 50;

                    let pos = shallow_copy(jbod_pos);

                    if (i < 5) {
                        pos.y += 100;
                    } else {
                        pos.y -= 10;
                    }


                    ctx.translate(pos.x + x, pos.y);
                    ctx.rotate(deg2rad(-90));

                    disk_box.draw(ctx);

                    ctx.restore();
                }
            });
        });

        objects.map((obj, i) => {
            ctx.save();

            ctx.globalAlpha *= getObjectAlpha(i, objects.length, params.t);
            obj.draw(ctx);

            ctx.restore();
        })
    });


    vid.add_object('jbod_main_wires', { opacity: 1, t: 0 }, (ctx, params) => {

        let front_fans_top = front_fans_box.top_center();
        front_fans_top.x += jbod_pos.x - nas_pos.x;

        let back_fans_top = back_fans_box.top_center();
        back_fans_top.x += jbod_pos.x - nas_pos.x;
        back_fans_top.y += -110;


        // front_fans_box.draw(ctx);
        // ctx.restore();

        // ctx.save();
        // ctx.translate(jbod_pos.x - nas_pos.x, -110);

        let lines = [
            [
                main_board_box.right_center(),
                new_psu_box.left_center()
            ],
            [
                main_board_box.bottom_center(),
                front_fans_top
            ],
            [
                main_board_box.bottom_center(),
                back_fans_top
            ],
        ];

        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 4;
        drawSequentialChains(ctx, lines, params.t);
    });

    vid.add_object('sas_wires1', { opacity: 1, t: 0 }, (ctx, params) => {

        let lines = [];

        for (var i = 0; i < 10; i++) {

            let pos = shallow_copy(jbod_pos);

            let x = ((i % 5) - 2) * 50;
            pos.x += x;

            if (i < 5) {
                pos.y += 100;
            } else {
                pos.y -= 10;
            }

            pos.y -= disk_box._width / 2;


            let pos2 = sas_expander_box.bottom_center();
            pos2.x += ((i % 5) - 2) * 5;

            lines.push([
                pos,
                pos2
            ]);
        }

        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 4;
        drawSequentialChains(ctx, lines, params.t);
    });

    vid.add_object('sas_wires2', { opacity: 1, t: 0 }, (ctx, params) => {

        let p1 = sas_hba_box.bottom_center();
        p1.y += 20;

        let p2 = shallow_copy(p1);
        p2.x = cpu_box.right_center().x;

        let lines = [
            [
                sas_expander_box.left_center(),
                sas_hba_box.right_center()
            ],
            [
                sas_hba_box.bottom_center(),
                p1
            ],
            [
                p1,
                p2
            ]
        ];

        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 4;
        drawSequentialChains(ctx, lines, params.t);
    })


    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'nas'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['nas_internals'], t, 1, { t: 1 });
    t += 1;
    t += pause;

    vid.add_transition(['jbod'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['jbod_disks'], t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['jbod_psu'], t, 0.5, { opacity: 1 });
    t += 0.5;
    vid.add_transition(['jbod_fans'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['jbod_main'], t, 0.5, { opacity: 1 });
    t += 0.5;
    vid.add_transition(['jbod_main_wires'], t, 0.5, { t: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['jbod_main_wires'], t, 0.5, { opacity: 0 });
    t += 0.5;
    t += pause;

    vid.add_transition(['jbod_main_wires'], t, 0.5, { opacity: 0 });
    t += 0.5;
    t += pause;

    vid.add_transition(['sas_cards'], t, 0.5, { opacity: 1 });
    t += 0.5;

    vid.add_transition(['sas_wires1'], t, 1, { t: 1 });
    t += 1;
    vid.add_transition(['sas_wires2'], t, 1, { t: 1 });
    t += 1;

    t += pause;

    vid.set_duration(t);

    return vid;

}

////////////////////////////////////////////////////////////////////////////////////

function part2_nas_video(canvas) {
    let vid = new Timeline();

    vid.set_name("part2_nas");

    vid.add_object('title', { opacity: 0, text: 'NAS Basics' }, (ctx, params) => {
        draw_title(ctx, params.text);
    });

    let body_grid = slide_body_grid(canvas);
    let box_grid = body_grid.split(3, 3);

    let nas_pos = body_grid.center();

    let nas_box = new DiagramBox({
        text: 'NAS',
        width: 300,
        height: 400,
        position: nas_pos,
        text_offset: {
            x: 0,
            y: -160,
        }
    });

    let computer_pos = shallow_copy(nas_pos);
    computer_pos.y -= 70;

    let computer_box = new DiagramBox({
        text: 'Computer',
        width: 250,
        height: 100,
        position: computer_pos,
    });

    let disk_pos = shallow_copy(nas_pos);
    disk_pos.y += 100;

    let disk_box = new DiagramBox({
        text: 'Disk',
        font_size: 18,
        width: 120,
        height: 40,
        position: { x: 0, y: 0 }
    });

    let router_pos = body_grid.center();
    router_pos.y = computer_pos.y;

    let router_box = new DiagramBox({
        text: 'Network\nRouter',
        width: 200,
        height: 100,
        position: router_pos
    });

    let nas_offset_x = box_grid.cell(0, 2).center().x - nas_pos.x;

    let desktop_pos = box_grid.cell(0, 0).center();
    desktop_pos.y = computer_pos.y;

    let desktop_box = new DiagramBox({
        text: 'Desktop PC',
        width: 200,
        height: 100,
        position: desktop_pos
    });

    let laptop_pos = box_grid.cell(1.75, 0).center();

    let laptop_box = new DiagramBox({
        text: 'Laptop',
        width: 200,
        height: 100,
        position: laptop_pos
    });


    vid.add_object('nas', { opacity: 0, offset_x: 0 }, (ctx, params) => {
        ctx.translate(params.offset_x, 0);
        nas_box.draw(ctx);
    });

    vid.add_object('computer', { opacity: 0, offset_x: 0 }, (ctx, params) => {
        ctx.translate(params.offset_x, 0);
        computer_box.draw(ctx);
    });

    vid.add_object('disks', { opacity: 0, offset_x: 0 }, (ctx, params) => {
        ctx.translate(params.offset_x, 0);

        for (var i = 0; i < 5; i++) {
            ctx.save();

            let x = (i - 2) * 50;

            ctx.translate(disk_pos.x + x, disk_pos.y);
            ctx.rotate(deg2rad(-90));

            disk_box.draw(ctx);

            ctx.restore();


            ctx.save();

            let line = [[
                { x: disk_pos.x + x, y: disk_pos.y - (disk_box._width / 2) },
                { x: disk_pos.x + x, y: computer_box.bottom_center().y }
            ]];

            ctx.strokeStyle = '#000';
            ctx.lineWidth = 2;
            drawSequentialChains(ctx, line, params.opacity);

            ctx.restore();
        }
    });

    vid.add_object('router', { opacity: 0 }, (ctx, params) => {
        router_box.draw(ctx);


        ctx.beginPath();

        let s = computer_box.left_center();
        s.x += nas_offset_x;

        let line = [[
            s,
            router_box.right_center(),
        ]];

        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;
        drawSequentialChains(ctx, line, params.opacity);
    });

    vid.add_object('desktop', { opacity: 0 }, (ctx, params) => {
        desktop_box.draw(ctx);

        let line = [[
            desktop_box.right_center(),
            router_box.left_center(),
        ]];

        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;
        drawSequentialChains(ctx, line, params.opacity);
    });
    vid.add_object('laptop', { opacity: 0 }, (ctx, params) => {
        laptop_box.draw(ctx);

        let line = [[
            laptop_box.right_center(),
            { x: router_box.bottom_center().x, y: laptop_box.right_center().y },
            router_box.bottom_center(),
        ]];

        ctx.strokeStyle = '#000';
        ctx.lineWidth = 2;
        drawSequentialChains(ctx, line, params.opacity);
    });

    vid.add_object('access', { opacity: 1, t: 0 }, (ctx, params) => {

        let disk_top = shallow_copy(disk_pos);
        disk_top.x += nas_offset_x;
        disk_top.x -= 50; // Disk spacing
        disk_top.y -= disk_box._width / 2;

        let computer_mid_pos = shallow_copy(computer_pos);
        computer_mid_pos.x = disk_top.x;


        let line = [[
            disk_top,
            computer_mid_pos,
            router_pos,
            { x: router_box.bottom_center().x, y: laptop_box.right_center().y },
            laptop_box.right_center(),
        ]];

        ctx.strokeStyle = '#f00';
        ctx.lineWidth = 4;
        drawSequentialChains(ctx, line, params.t);
    });

    let pause = 0.5;

    let t = 0;

    vid.add_transition(['title', 'nas'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['computer'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['disks'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['nas', 'computer', 'disks'], t, 0.5, { offset_x: nas_offset_x });
    t += 0.5;

    vid.add_transition(['router'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['desktop', 'laptop'], t, 0.5, { opacity: 1 });
    t += 0.5;
    t += pause;

    vid.add_transition(['access'], t, 1, { t: 1 });
    t += 1;
    t += pause;


    vid.set_duration(t);

    return vid;
}