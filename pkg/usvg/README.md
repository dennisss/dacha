μsvg
====

A node.js based svg optimizer built on top of svgo with more aggressive minification features

List of features
- Whatever SVGO does
- Visual verification
	- A test render of the svg is done and a similarity check is done at a configurable accuracy level to verify that the svg was not compressed too aggressively
- Minification of id/class names and removal of unused id/class names
- Invisible element removal
	- This will remove all elements in the svg that are completely hidden behind other elements or are outside the viewport and thus don't contribute to the pixels rendered on screen.
