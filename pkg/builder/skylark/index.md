# Skylark Interpreter

This library continues a parser and interpreter for executing Skylark (a subset of Python) code.
We base the grammar and functionality on the Bazel spec defined
[here](https://github.com/bazelbuild/starlark/blob/master/spec.md).

Memory management is done using a simple mark and sweep garbage collector. Cyclic references are
fully supported.