/*
node -r esm pkg/media/creation/animation/js/node.js
*/

const { createCanvas, loadImage, registerFont } = require('canvas')
const { encode_frames } = require('./video_encoder');
const { configure } = require('./timelines/jbod/jbod_animation');



registerFont('third_party/noto_sans/font_normal.ttf', { family: 'Noto Sans' });
registerFont('third_party/noto_sans/font_mono_normal.ttf', { family: 'Noto Sans Mono' });
registerFont('third_party/noto_color_emoji/NotoColorEmoji-Regular.ttf', { family: 'Noto Color Emoji' });


const canvas = createCanvas(3840, 2160)
const ctx = canvas.getContext('2d');

ctx.antialias = 'subpixel';

let inner_canvas = { width: 960, height: 540 };
ctx.scale(4, 4);


const FRAME_RATE = 29.97;

class Frames {
    constructor(timeline) {
        this._i = 0;
        this._timeline = timeline;
    }

    width() {
        return canvas.width;
    }

    height() {
        return canvas.height;
    }

    length() {
        return Math.round(this._timeline.duration() * FRAME_RATE);
    }

    rate() {
        return FRAME_RATE;
    }

    next() {
        let time = this._i / FRAME_RATE;
        this._i += 1;
        this._timeline.draw(inner_canvas, ctx, time);
        return canvas.toBuffer('raw'); // BRGA pixel buffer
    }
}

(async () => {
    let timeline = await configure(inner_canvas);
    if (!timeline.name()) {
        throw new Error('Unnamed timeline');
    }

    let frames = new Frames(timeline);

    await encode_frames(frames, `dump/${timeline.name()}.mp4`);
})()
