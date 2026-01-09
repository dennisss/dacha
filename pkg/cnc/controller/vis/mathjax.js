// Based on
// https://docs.mathjax.org/en/latest/web/convert.html
//
// And 
// https://github.com/mathjax/MathJax-node

const BROWSER_MODE = typeof window != 'undefined';

const svgCss = [
    'svg a{fill:blue;stroke:blue}',
    '[data-mml-node="merror"]>g{fill:red;stroke:red}',
    '[data-mml-node="merror"]>rect[data-background]{fill:yellow;stroke:none}',
    '[data-frame],[data-line]{stroke-width:70px;fill:none}',
    '.mjx-dashed{stroke-dasharray:140}',
    '.mjx-dotted{stroke-linecap:round;stroke-dasharray:0,140}',
    'use[data-c]{stroke-width:3px}'
].join('');
const xmlDeclaration = '<?xml version="1.0" encoding="UTF-8" standalone="no"?>';
const SVGXMLNS = 'http://www.w3.org/2000/svg';

if (!BROWSER_MODE) {
    var mjAPI = require("mathjax-node");
    mjAPI.config({
        MathJax: {}
    });
    mjAPI.start();
}

export async function math_to_svg(math, options = {}) {
    if (BROWSER_MODE) {
        const adaptor = MathJax.startup.adaptor;
        const result = await MathJax.tex2svgPromise(math, options);
        const svg = adaptor.tags(result, 'svg')[0];
        const defs = adaptor.tags(svg, 'defs')[0] || adaptor.append(svg, adaptor.create('defs'));
        adaptor.append(defs, adaptor.node('style', {}, [adaptor.text(svgCss)], SVGXMLNS));
        adaptor.removeAttribute(svg, 'role');
        adaptor.removeAttribute(svg, 'focusable');
        adaptor.removeAttribute(svg, 'aria-hidden');
        const g = adaptor.tags(svg, 'g')[0];
        adaptor.setAttribute(g, 'stroke', 'black');
        adaptor.setAttribute(g, 'fill', 'black');
        return xmlDeclaration + '\n' + adaptor.serializeXML(svg);
    } else {
        var mjAPI = require("mathjax-node");

        return new Promise((res, rej) => {
            mjAPI.typeset({
                math: math,
                format: 'TeX',
                svg: true,
            }, function (data) {
                if (data.errors) {
                    rej(data.errors);
                } else {
                    res(data.svg);
                }
            });
        });
    }
}

export function math_scale() {
    if (BROWSER_MODE) {
        return 1;
    } else {
        return 4;
    }
}

export async function math_to_img(math) {
    let svg = await math_to_svg(math);

    if (BROWSER_MODE) {
        const img = new Image();

        let p = new Promise((res, rej) => {
            img.onload = () => {
                res();
            }
            img.onerror = () => {
                console.log('Load failure');
                console.log(svg);
            }
        });

        let svg_url = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
        img.src = svg_url;

        await p;

        return img;

    } else {
        const { loadImage } = require('canvas');
        let img = await loadImage(Buffer.from(svg));

        // The node-canvas code is bad https://github.com/Automattic/node-canvas/issues/1474
        img.width *= 4;
        img.height *= 4;

        return img;
    }
}