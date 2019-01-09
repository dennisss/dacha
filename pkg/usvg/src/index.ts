

import { JSDOM } from 'jsdom';
import fs from 'fs';
import path from 'path';
import Image from './image';
import SVGO from 'svgo';
import csstree from 'css-tree';
import { selfCloseTags, traverseChildren, isElement } from './utils';


// TODO: On web, this could be implemented purely in terms of the canvas
// NOTE: we don't use node-canvas because it's svg rendering support is pretty rubish and easily misconfigured

// TODO: If an svg contains a raster image, we may also be able to optimize it (especially in terms of clipping it if it is being clipped)
// ^ This will also require proper understanding knowledge of what resolution everything is operating at


interface OptimizeOptions {
	/**
	 * If set, extra images and files will be emitted to this directory for debugging intermediate output of the optimizer
	 */
	debugOutputDir?: string; 
}


export default class uSVG {

	public constructor() {

	}

	/**
	 * Minifies an svg
	 * 
	 * @param input_svg The raw string representation of the svg
	 */
	public async optimize(input_svg: string, options: OptimizeOptions = {}): Promise<string> {
		
		let { window } = new JSDOM(input_svg);
		let root = window.document.getElementsByTagName('svg')[0];
	
		let state = new uSVGState(input_svg, root, options);
		return await state.output();
	}


	public async optimizeFile(filename: string, options?: OptimizeOptions): Promise<string> {
		let str = fs.readFileSync(filename).toString('utf8');
		return await this.optimize(str, options);
	}

}

interface RenderOutput {

}

interface IdentCount {
	// Number of times that this identifier is defined (by a 'class' or 'id' attribute) 
	defs: number;

	// Number of times it is referenced. By a style rule, href, etc.
	refs: number;
}

/**
 * Encapsulates a single optimization instance for a single svg
 */
class uSVGState {

	constructor(private input_svg: string, private root: SVGSVGElement, private options: OptimizeOptions) {

	}

	public async output(): Promise<string> {

		// TODO: We need to control the output size to some minimum
		let current = await this.renderCurrent();
		let original = current;
		//console.log(`Dimensions: ${current.image.width} x ${current.image.height}`);

		// Verify that we can reproduce the input by serializing and normalizing the parsed tree
		if(current.raw !== this.input_svg) {
			throw new Error('Inconsistent outer html of root node');
		}

		// TODO: This must be tested to verify that the image looked correct
		if(this.options.debugOutputDir) {
			await current.image.save(path.join(this.options.debugOutputDir, 'input.jpg'));
		}


		let idx = 0;

		// NOTE: This should also take care of stripping the '<title>' tags
		await traverseChildren(this.root, async (node, parent) => {

			// Remove from tree
			let nextSibling = node.nextSibling;
			parent.removeChild(node);

			// Get new image
			let next = await this.renderCurrent();

			// For debugging incremental changes
			/*
			if(options.debugOutputDir) {
				next.image.save(path.join(options.debugOutputDir, `${idx++}.jpg`));
			}
			*/

			// Compare before and answer the node is disabled
			// TODO: It would probably be sufficient to just get a raw count of pixels changed
			let score = await current.image.similarity(next.image);

			// TODO: Probably scale this margin based on how many pixels are in
			let isMeaningful = score < 0.9999;

			//console.log(isElement(node)? node.tagName : '[unknown node]', score);

			// Add back meaningful nodes
			if(isMeaningful) {
				parent.insertBefore(node, nextSibling);
			}
			// Keep meaningless ones removed
			else {
				//console.log('- Removing');
				current = next;
				return false;
			}

			return true;
		});

		await traverseChildren(this.root, async (node, parent) => {
			if(!isElement(node)) {
				return true;
			}

			for(let name of node.getAttributeNames()) {
				if(name.toLowerCase().indexOf('data-') === 0) {
					node.removeAttribute(name);
				}
			}
			
			return true;
		});

		// TODO: Visually validate that this has not changed the appearance of the svg
		await this.optimizeSelectorNames();

		// Re-update with new changes from the later filters
		current = await this.renderCurrent();

		// Testing overall difference made by all optimizations
		let changeScore = await current.image.similarity(original.image);
		if(changeScore < 0.99) {
			console.log(changeScore);
			throw new Error('Loss-less optimizations changed image appearance')
		}

		let output = current.raw;

		// Finally perform regular optimizations
		output = (await (new SVGO()).optimize(output)).data;

		// TODO: Check the SVGO image similarity as well

		if(this.options.debugOutputDir) {
			current.image.save(path.join(this.options.debugOutputDir, 'final.jpg'));
			fs.writeFileSync(path.join(this.options.debugOutputDir, 'final.svg'), output);
		}

		return output;
	}

	async optimizeSelectorNames() {

		// These will become the final mapping of old names to new shortened names
		let classMap: { [name: string]: string } = {};
		let idMap: { [name: string]: string } = {};

		let styleEls: HTMLStyleElement[] = [].slice.call(this.root.getElementsByTagName('style'));
		let cssTrees = styleEls.map((el) => {
			return csstree.parse(el.textContent || '');
		});

		// All classes and ids referenced with the number of times they are referenced
		// TODO: If a class/id is used, then at least one xlink:ref
		// For each we must know the number of defs and the number of refs
		let classNames: { [name: string]: IdentCount; } = {};
		let idNames: { [name: string]: IdentCount; } = {};

		function incRefCount(obj: { [name: string]: IdentCount; }, name: string) {
			if(!obj[name]) { obj[name] = { defs: 0, refs: 0 }; }
			obj[name].refs++;
		}

		function incDefCount(obj: { [name: string]: IdentCount; }, name: string) {
			if(!obj[name]) { obj[name] = { defs: 0, refs: 0 }; }
			obj[name].defs++;
		}

		// Extracts/repalces identifier usage from xml elements
		// TODO: Currently this doesn't account for cases of refs and defs to same identifier on the same element (but those should be uncommon anyway)
		// TODO: Should we deal with removing empty string attributes?
		let traverseNodes = async (replacing: boolean) => {
			await traverseChildren(this.root, async (node) => {
				if(!isElement(node)) {
					return true;
				}
	
				let id = node.getAttribute('id');
				if(id) {
					if(replacing) {
						if(idMap[id]) { node.setAttribute('id', idMap[id]); }
						else { node.removeAttribute('id'); }
					}
					else {
						incDefCount(idNames, id);
					}
					
				}
	
				let className = node.getAttribute('class');
				if(className) {
					if(replacing) {
						if(classMap[className]) { node.setAttribute('class', classMap[className]); }
						else { node.removeAttribute('class'); }
					}
					else {
						incDefCount(classNames, className);
					}	
				}
	
				['href', 'xlink:href'].map((attr) => {
					let href = node.getAttribute(attr);
					if(!href) {
						return;
					}

					href = href.trim();

					if(href[0] === '#') {
						let h = href.slice(1);
						if(replacing) {
							if(idMap[h]) { node.setAttribute(attr, '#' + idMap[h]); }
							else { node.removeAttribute(attr); }
						}
						else {
							incRefCount(idNames, h);
						}
					}
				});
	
				return true;
			});
		}


		let traverseCss = async (replacing: boolean) => {

			let somethingRemoved = false;
			let cssWalker = (node: csstree.CssNode) => {
				if(node.type === 'IdSelector') {
					let id = node.name;
					if(replacing) {
						if(idMap[id]) { node.name = idMap[name]; }
						else {
							// TODO: Must remove the whole rule
							node.name = 'REMOVED';
							somethingRemoved = true;
						}
					}
					else {
						incRefCount(idNames, id);
					}
				}
				else if(node.type === 'ClassSelector') {
					let className = node.name;
					if(replacing) {
						if(classMap[className]) { node.name = classMap[className]; }
						else {
							// TODO: Must remove the whole rule
							node.name = 'REMOVED';
							somethingRemoved = true;
						}
					}
					else {
						incRefCount(classNames, className);
					}
				}
				else if(node.type === 'Url') {
					let u = node.value.value.trim();
					if(u[0] === '#') {
						let id = u.slice(1);
						if(replacing) {
							if(idMap[id]) { node.value.value = '#' + idMap[id]; }
							else {
								// TODO: Entire rule must be removed (at least just the selectors applicable to this)
								node.value.value = '#REMOVED';
								somethingRemoved = true;
							}
						}
						else {
							incRefCount(idNames, id);
						}
					}
				}
			}


			cssTrees.map((t) => {
				if(replacing) {

					// Removing individual selector cases
					csstree.walk(t, {
						visit: 'Selector',
						enter: (node, item, list) => {
							somethingRemoved = false;
							csstree.walk(node, cssWalker);
							if(somethingRemoved && list) {
								list.remove(item);
							}
						}
					});

					csstree.walk(t, {
						visit: 'Declaration',
						enter: (node, item, list) => {
							somethingRemoved = false;
							csstree.walk(node, cssWalker);
							if(somethingRemoved && list) {
								list.remove(item);
							}
						}
					});

					// Removing all the rules that we ended up internally removing because of the above cases
					// Basically an empty-rule removal walking run
					csstree.walk(t, {
						visit: 'Rule',
						enter: (node, item, list) => {
							if(node.type !== 'Rule') {
								return;
							}

							// TODO: Will the prelude ever not be a SelectorList?
							let empty = node.prelude.type === 'SelectorList'
								&& node.prelude.children.getSize() === 0;

							empty = empty || node.block.children.getSize() === 0;

							if(empty && list) {
								list.remove(item);
							} 
						}
					})


					// TODO: Run a pass other everything else (NOTE: We can't repass on any nodes that we have already seen as that would attempt to remangle everything)
				}
				else {
					csstree.walk(t, cssWalker);
				}
			})
		}


		// Extract all identifier counts
		await traverseNodes(false);
		await traverseCss(false);
		
		// Generate new mappings
		function minimalSortedNamed(obj: { [name: string]: IdentCount; }): string[] {
			return Object.keys(obj).filter((n) => {
				// Take only those that are meaninfully used in a pair
				return obj[n].refs > 0 && obj[n].defs > 0;
			}).sort((a, b) => {
				// Sorting in descending order of total occurence counts
				return (obj[b].refs + obj[b].defs) - (obj[a].refs + obj[a].defs);
			})
		}

		function createMapping(obj: { [name: string]: IdentCount; }): { [name: string]: string } {
			let out: { [name: string]: string } = {};
			let gen = IdentifierGenerator();
			minimalSortedNamed(obj).map((name) => {
				let newName = gen.next().value;
				out[name] = newName;
			});

			return out;
		}

		idMap = createMapping(idNames);
		classMap = createMapping(classNames);
		

		// Now we put everything back in
		await traverseNodes(true);
		await traverseCss(true);

		// Re-serialize the css now
		cssTrees.map((t, i) => {
			let str = csstree.generate(t)

			//if(str.indexOf('REMOVED') >= 0) {
			//	throw new Error('Css mangling has failed');
			//}

			// TODO: Now empty style elements can be removed completely
			styleEls[i].textContent = str;
		});
	}


	// Helper for getting the image and svg representation of the in-memory DOM tree declared in 'root'
	async renderCurrent() {
		let ts = new Date();
		let out = selfCloseTags(this.root.outerHTML);
		let img = await Image.read(Buffer.from(out, 'utf8'));
		let te = new Date();
		
		// TODO: Currently this seems really slow
		//console.log('Render took ' + (te.getTime() - ts.getTime()) + 'ms');

		return { image: img, raw: out };
	}



}


// Will generate sequential increasingly long non-overlapping case-insensitive class/id names
function *IdentifierGenerator() {
	// Represents the set of all valid identifiers in css classNames/ids
	// See https://stackoverflow.com/questions/448981/which-characters-are-valid-in-css-class-names-selectors
	let valid_regex = /^-?[_a-zA-Z]+[_a-zA-Z0-9-]*$/;

	let num = 0;

	while(true) {
		let s = num.toString(32).toLowerCase();
		if(valid_regex.exec(s)) {
			yield s;
		}

		num++;
	}
}


