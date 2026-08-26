# Optical Motion Capture : Host Software Design

This page covers important details about the design of the software running on the host machine to combine the data from many cameras.

The code for the entry point in the user application is in `//pkg/vision/mocap/app` and is what we are focused on describing here.

## Data Storage

The app does need to store some data for tracking the config and state of the cameras across restarts so all application data is stored in one of the following `DATA_DIR` locations depending on the OS:

- Linux: `$HOME/.local/share/mocap`
- macOS: `$HOME/Library/Application Support/Mocap`
- Windows: `C:\Users\<Username>\AppData\Local\Mocap`

We do not use a separate config and data directory for simplicity.

## Configs

All the configuration for the host software is encapsulated in a `MocapManagerConfig` proto. Not all fields in thie config are stable and some may be added/removed/renamed after time so we need to balance letting users tweak everything vs preserving settings across software updates without errors.

How this proto is read by the app follows a layered approach:

- The default values are stored in [//pkg/vision/mocap/config/manager.txtpb](/pkg/vision/mocap/config/manager.txtpb) in the code repository.
    - This file is embedded in the app.
- `DATA_DIR/config_base.pbtxt`
    - This file is initialized from the previous source when the application starts.
    - It is reset whenever there is an update to the base config file.
    - If a power user wants contorl over every possible setting available, they should edit this file under the disclaimer that they will need to manaully merge the changes into the new config if the software is updated.
- `DATA_DIR/config.pb`
    - This contains calibration data and settings that were configured via the app UI.
    - This file is 'merged' on top of the base config to form the final config used by the app.
    - Fields used by this file (exposed in the UI) aim to be stable across software updates.

## UI

The UI is currently web based (written in HTML/JavaScript). When the host software opens, it spawns a [web view](/pkg/web/view/index.md) using whatever browser runtime is available on the host machine.

Communication with the UI is done by starting a HTTP server on a random port and having the web view send HTTP1 and websocket requests. Note that it is possible to do everything over IPC but some operating systems (GTKWebkit on Linux) have very small IPC connections so HTTP is better and allows standardizing the code base for usecases that are headless and want to use a detached UI accessible from a separate web browser. Also note that HTTP1 has limited concurrency in most browsers so websockets or HTTP2 are needed to have good multiplexing but HTTP2 is only easy to get working if using TLS.