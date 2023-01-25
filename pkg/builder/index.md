# Builder

Tool for building things (similar to Bazel or Buck).

## Terminology

- `BUILD` file: File stored in each source code directory and written in Skylark which defines a package's contents.
- `Package`: Set of `Target`s.
- `Target`: Discrete buildable component which:
    - Is identified by a `Label`
    - Has a set of `Target` dependencies that must be build first.
    - Is executable and can produce a set of output files and other metadata.
    - Normally these are instantiated by `Rule`s executed in the `BUILD` file.
    - Every instance of a `Target` is defined in the context of a `BuildConfig`.
        - It is possible that for two different `BuildConfigs` a target may perform the exact same operations when given the same inputs.
        - We will determine this by hashing the `Target` instance.
- `Rule`: A function which takes as input a set of attributes and the current `BuildConfig` and produces a `Target` instance.
    - A rule's job is basically to fully extract a 
- `(Build)Config`: Set of 'global' config parameters which should be used when executing `Target`s.
    - We can think of a config as being generally comprised of:
        - A `Platform` configuration: defines rule values to use for a specific chip/host architecture.
        - A `Profile` confiuration: e.g. debug vs release or dev vs prod.

    - Challenge is that stuff like genrules can't easily have different dependencies for different architectures unless we either:
        - 

Passing a `BuildConfig` to genrules scripts:
    - Provide a BUILD_CONFIG environment variable which is a path to the current proto in use.

In bazel the configuration is essentially a `map<Label, String>`.

- Can we use a config file in order to conditionally use a different SVD file as a dependency.

```
genrule(
    name = "peripherals_generated",
    srcs = [
        "//pkg/svd:svd_generator",
        "//third_party/"
    ],
    outs = ["nrf52840.rs"],
    cmd = "$(location //pkg/svd:svd_generator),"
)
```

TODO: If two rules perform the same operation and the only difference is the output file name, we should be able to deduplicate them.

TODO: Normalize the targets referenced in TargetConfig protos so that we don't need to recompile targets if just labels stay semantically the same.

BuildConfig design:
- Should the BuildConfig be slim or contain a lot of settings like memory layout?
    - Obviously some stuff like the SVD file doesn't need to go there
    - If it isn't in the BuildConfig, it can be referenced from some map like elsewhere.
- Simple solution is to have arbitrary labels like "cortex_m", "cortex_m4" "nrf5", "nrf52", "nrf52840"
    - Based on that we can look up different types of 

- da build //pkg/builder:main --config=//pkg/builder/config:rpi_release

- A config could probably still perform conditional logic 

- Challenge: Map fields do complete value overwriting.
    - We will define higher order merging semantics.
- A challenge with



Step 1:
- Loading:
    - Scan through all build files and instantiate the rules
    - Each rule should be able to enumerate its dependencies (potentially conditioned on a specific )
- Execute every rule with others as input to produce the operation graph


Challenges:
- For a build rule to run, it would need to know the path to inputs
    - This implies that input rules must have already been executed.
- In Bazel, all the rule definitions are first accumulated
    - I assume that 

- so we can run the macros,
    - Analysis: That will just get us a set of configured rules which we need to get the dependencies of
    - Them we need to also build all those dependency files.
    - Loading: Actually execute the rules with outputs from other rules defined
- why is this nice?
    - Allows us to do stuff like creating separate actions for each file returned by the input rueles


Design:
- To build anything, you first need to have a build configuration
    - When the builder starts running, it will bootstrap the config with suitable defaults for the current machine
- We do need to distinguish between the 'host' config and the 'target' config
    - Any binaries required for the purposes of building files (like protobufs) must be runnable on the builder machine
- We want to be able to analyze the build graph before anything is executed 
- First thing that happens is we evaluate the BUILD files with the default config.
- BuildConfig protos are all registered as separate targets in the build system which output a BuildConfig binary proto file.
    - So every BuildConfig can be identified by a name
    - For caching, we also need to hash the config
    - Also we may want to support derived configs?

- Any BUILD file which doesn't request to get environment variables can be cached.

- Some definitions
    - Rule: f(config, attributes) -> actions + inputs + outputs graph

- Action execution:
    - Each action has a Run() function to which we pass a protobuf with:
        - The action config + requested inputs + a place to write any outputs.
- Action Input:
    - Described by:
        - Label and Config of the target to build to generate this input
        - A name (all file input/outputs in the same package must have unique names)
- Example Action: gzip
    - Expect to receive one input: A FileInput (containing at least a path to )
    - Writes the the data to a single output 'tag'

/*
TODO: Consider using the bazel model of a target being comprised of a list of actions/operations that need to be applied.
*/



- The whole repository is composited of build rules which are each identified by a 

- To build anything you need to 


/*
GzipRule(attributes, context) {

    input = context.input(attributes.input, config=attributes.input_config)

    op = context.Operation(
        type =
        inputs = [input]
    )
}


Defining an operation:
    - name: Must be unique across all operation instances in the

    - inputs:
        - Each input is identified by (target_label, op_name, config, output_key)
            - When op_name is not specified we are referring to the output of the entire targt
            - An input or list of outputs is mapped to a single key
        So an op must do:
            - input_file = context.input::<BuildOutputFile>("input")
            - output_file = context.output::<BuildOutputFile>("output")
            - // Perform operations to read from input_file and write to output_file
            -

            - Within the same rule, we can use

*/

/*
We execute a
*/

/*
- BuildRuleFactory
    ::create(&Any) -> Result<Box<dyn BuildRule>>
- BuildRule
    - ::name()
    - ::query_deps() : Advisory
    - ::query_inputs()
    - ::query_outputs()
*/




Our execution model is effectively:

1. Execute rules as normal with a 'native' config to produce the BuildConfig
2. Use the config to instantiate configs which we can 



Files
- `built/` contains the contacts of the 
- `built-config/[config_name]/`

- `//builder/config:raspberry_pi`

- `built-config/raspberry_pi-443did81a`

- `built-rust/`



Step 1:
- Need to know which profile is in use

- 


`cross build --target=armv7-unknown-linux-gnueabihf --bin cluster_node --release --target-dir=/home/dennis/workspace/dacha/built-rust/1`


/*
rustc --crate-name usb_device --edition 2018 --crate-type rlib -o build/test.rlib pkg/usb_device/src/lib.rs

*/


Leverage BUILD files.
- For now, auto-add binaries for Cargo projects.


/*
    Every build runs with an ambient:
    - BuildConfig {}

    - Target specific parameters take priority
    - Followed by bundle configs
    - Followed by user provided flags.

    - So yes, we can build a single rule with many different settings
*/


Specifying the sources for a bundle:
- All targets can emit some files that it produces.
    - e.g. binary, intermediate files like compiled proto code, static files, etc.
- Only some should be added to bundles 
    - (imtermediate files should normally not be added)
- But, we also want to support nested dependencies.
    - E.g. 

- To build a bundle,
    - User specified 'deps'
    - Can be either direct dependencies or 