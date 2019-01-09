

/**
 * Convert '<x ...></x>' to '<x .../>'
 */
export function selfCloseTags(xml: string) {
	return xml.replace(/<([a-z]+)([^>]+)><\/\1>/g, function(whole: string, tagName: string, tagAttrs: string){
		return `<${tagName}${tagAttrs}/>`;
	});
}

/**
 * Traverses all DOM Nodes within the given element.
 * 
 * The given function will be given arguments of (currentNode, parentNode)
 * If the given function returns 'false', then we will stop traversing down that route
 */
export async function traverseChildren(el: Node, fn: (cur: Node, parent: Node) => Promise<boolean>) {
	let children = [].slice.call(el.childNodes);
	for(let c of children) {
		let res = await fn(c, el);
		if(res === false) {
			continue;
		}

		await traverseChildren(c, fn);
	}
}

export function isElement(node: Node): node is Element {
	return node.nodeType === 1;
}