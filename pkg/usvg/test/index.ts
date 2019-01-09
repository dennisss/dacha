import uSVG from '../src/index';
import path from 'path';
import mkdirp from 'mkdirp';

describe('uSVG', () => {

	it('hidden-paths sample', async () => {

		let inst = new uSVG();

		let inputFile = path.join(__dirname, '../samples/hidden-paths/input.svg');
		let debugDir = path.join(__dirname, '../out/hidden-paths');
		mkdirp.sync(debugDir);

		let out = await inst.optimizeFile(inputFile, { debugOutputDir: debugDir });


	});


});