

import axios, { AxiosResponse } from 'axios';
import fs from 'fs';
import path from 'path';
import mkdirp from 'mkdirp';

const dataDir = path.resolve(__dirname, '../../data/picsum');

mkdirp.sync(dataDir);


interface PicsumItem {
	format: string;
	width: number;
	height: number;
	filename: string;
	id: number;
	author: string;
	author_url: string;
	post_url: string;
}

async function main() {

	let listRes = await axios('https://picsum.photos/list');
	let list = listRes.data as PicsumItem[];

	console.log(list.length);

	for(let i = 0; i < list.length; i++) {
		console.log(i + ' / ' + list.length)
		let it = list[i];

		let imgRes: AxiosResponse<any>;
		try {
			imgRes = await axios({
				method: 'GET',
				url: `https://picsum.photos/${it.width}/${it.height}?image=${it.id}`,
				responseType: 'stream'
			});
		}
		catch(err) {
			console.log('- Failed!');
			continue;
		}
		

		let outFile = path.join(dataDir, it.id + '.' + it.format);
		if(fs.existsSync(outFile)) {
			console.log('- Already downloaded');
			continue;
		}

		let out = fs.createWriteStream(outFile);
		imgRes.data.pipe(out);

		await new Promise((res, rej) => out.once('finish', res));
	}

}


if(require.main === module) {
	main().catch((err) => {
		console.error(err);
	});
}
