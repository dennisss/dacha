# Kicad Integration Utilities

This package contains shared utilities for working with KiCad (managing component libraries, exporting PCB fabrication files, etc.)

## Developing

All PCBs in this repository currently use KiCad 7 format files and only reference symbols/footprints either in the builtin KiCad libraries or those in this repository. To avoid simplify component library management, all symbol/footprint libraries need to be defined (via the below command) globally rather than per kicad project.

If you haven't already, ensure that you have KiCad 9 installed. If you don't have it installed, install with:

```
sudo add-apt-repository ppa:kicad/kicad-9.0-releases
sudo apt update

sudo apt install kicad
```

Then run the following command to setup your user level symbol/footprint libraries to reference those in this repository:

```
cargo run --bin kicad_library_setup
```

This reads entries from the [config/libraries.txtpb](./config/libraries.txtpb) file where each entry corresponds to up to 1 symbol library and 1 footprints library stored in the same directory.

## Settings

The following KiCad settings are recommended:

- Disable 'Automatically backup projects' in the KiCad preferences.
- Install plugins:
    - https://github.com/bennymeg/Fabrication-Toolkit
    - https://github.com/openscopeproject/InteractiveHtmlBom

## Style Guide

- Each PCB revision corresponds to a single KiCad project directory in the repository. Typically a package will have a `boards` direcotry to group together all the PCBs associated with.
    - Each individual project directory should be named like `[board-name]/latest` or `[board-name]/r1`.
    - Typically development should happen in the `latest` directory and then once it is ready to go to production, we will fork/freeze it into the next `r1`, `r2`, etc. directory while also adding a silkscreen label to the PCB to indicate the revision and board name.

- Always use 1.27mm grid for laying out schematics.
