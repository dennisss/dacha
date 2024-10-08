import { decode_le_u32, decode_utf8 } from "pkg/web/lib/encoding";

export interface BinaryImageData {
    width: number;
    height: number;
    data: any;
}

export function parse_binary_images(buf: ArrayBuffer): BinaryImageData[] {
    let out = [];

    let i = 0;
    while (i < buf.byteLength) {
        let magic = decode_utf8(new Uint8Array(buf, i, 4));
        i += 4;

        if (magic != 'daBI') {
            throw new Error('Wrong magic bytes');
        }

        // Version/flags [1, 0, 0, 0]
        // TODO: Check this.
        i += 4;

        let height = decode_le_u32(new Uint8Array(buf, i, 4));
        i += 4;

        let width = decode_le_u32(new Uint8Array(buf, i, 4));
        i += 4;

        let len = Math.floor(height * Math.ceil(width / 8));

        console.log(i, i + len, buf.byteLength);

        let pixel_data = new Uint8Array(buf, i, len);
        i += len;

        out.push({
            width, height, data: pixel_data
        });
    }

    return out;
}

export function binary_image_to_image(image: BinaryImageData, color: number[]) {
    let pixel_data = new Uint8ClampedArray(4 * image.height * image.width);

    let input: Uint8Array = image.data;
    let i = 0;
    let j = 0;

    let count = 0;
    for (var a = 0; a < input.length; a++) {
        if (input[a] != 0) {
            count += 1;
        }
    }

    for (let y = 0; y < image.height; y++) {
        for (let x = 0; x < image.width; x++) {

            let bits = input[i];
            let bit = (bits >> (7 - (x % 8))) & 1;

            // TODO: Paint different colors per tool.
            pixel_data[j + 0] = color[0]; // R value
            pixel_data[j + 1] = color[1]; // G value
            pixel_data[j + 2] = color[2]; // B value
            pixel_data[j + 3] = bit ? color[3] : 0; // A value

            if ((x + 1) % 8 == 0) {
                i += 1;
            }

            j += 4;
        }

        if (image.width % 8 != 0) {
            i += 1;
        }
    }

    if (i != input.length) {
        throw new Error('Not all read');
    }

    let image_data = new ImageData(pixel_data, image.width, image.height);

    {
        var canvas = document.createElement('canvas');
        canvas.width = image_data.width;
        canvas.height = image_data.height;

        var ctx = canvas.getContext('2d');
        ctx.putImageData(image_data, 0, 0);


        return canvas;

    }

    return image_data_to_image(image_data);
}


function image_data_to_url(image_data: ImageData): string {
    var canvas = document.createElement('canvas');
    canvas.width = image_data.width;
    canvas.height = image_data.height;

    var ctx = canvas.getContext('2d');
    ctx.putImageData(image_data, 0, 0);


    // return canvas.toDataURL('image/png');
}

function image_data_to_image(image_data: ImageData) {
    let img = new Image();
    img.src = image_data_to_url(image_data);
    return img;
}
