# Optical Motion Capture : Host Software Design

This page covers important details about the design of the software running on the host machine to combine the data from many cameras.

The code for the entry point in the user application is in [//pkg/vision/mocap/app](/pkg/vision/mocap/app/) and is what we are focused on describing here.

## Data Storage

The app does need to store some data for tracking the config and state of the cameras across restarts so all application data is stored in one of the following `DATA_DIR` locations depending on the OS:

- Linux: `$HOME/.local/share/mocap`
- macOS: `$HOME/Library/Application Support/Mocap`
- Windows: `C:\Users\<Username>\AppData\Local\Mocap`

We do not use a separate config and data directory for simplicity.

## Configs

All the configuration for the host software is encapsulated in a [MocapManagerConfig](/pkg/vision/mocap/proto/manager.proto) proto. Not all fields in thie config are stable and some may be added/removed/renamed after time so we need to balance letting users tweak everything vs preserving settings across software updates without errors.

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

## Building

The final distributed binaries are built using

```bash
./pkg/vision/mocap/app/build.sh
```

For development, the following two commands can be useful:

```bash
# Rebuild the UI code (will be enabled when the webview is refreshed)
cargo run --bin builder -- build //pkg/vision/mocap/manager:app

# Rebuild and run the app (with dev tools enabled so can be live refreshed with UI changes)
cargo run --bin mocap_app -- --enable_devtools
```

## Design

All the core code for the host software is located in [//pkg/vision/mocap/manager](/pkg/vision/mocap/manager).

- As described in [this page](./networking.md), the software connects to the cameras via link local connections + mDNS
- The manager calls the `ReadBlobs` RPC on all cameras and waits for all cameras to return results at the same timestamps.
- Then the [BlobMatcher](/pkg/vision/mocap/manager/src/matching.rs) does epipolar matching and triangulation to convert 2D points to 3D
    - This also includes the alpha-beta filter for velocity based tracking of old points.
- Then the [RigidBodyTracker](/pkg/vision/mocap/manager/src/rigid_body.rs) performs the point cloud analysis to detect and track rigid bodies.

More details on individual elements are available below.

### Blob Matching

The `BlobMatcher` is configured by the `matching` field in the config (see the [BlobMatchingConfig](/pkg/vision/mocap/proto/matching.proto) docs). The input to this stage are the 2d points from each camera and the outputs are 3d points. The conversion process generally works as follows:

- For each 2D point 'A'
    - Try to find matching points in other cameras using epipolar matching (the second point must match within `max_reprojection_error` pixels of the epipolar line)
    - For each of these matching points 'B'
        - Triangulate a 3d point using 'A' and 'B'
        - Attempt to find more matches in other camera views by directly projecting the 3D points into other camera views and finding the nearest 2d point (must be within `max_reprojection_error` pixels of the guess).
        - Add the set of all matches 2d points to the 'proposals' list.
- Once we have a list of 'proposals', we sort it by the # of involved cameras and greedily output 3D points started using the proposal with highest # of cameras.
    - Note that a proposal must involve at least `min_num_matches` cameras for it to be accepted.
- The final matched 3d points are saved internally and will be used the next time the matcher runs (this happens before the above logic runs):
    - For each old 3d point,
        - Use an alpha-beta predictor to estimate the new 3D position of the point.
        - Directly project this 3D point into all cameras to try and find a 'proposal'
            - For old points, the proposals only need `min_num_rematches` cameras to match for a point to be generated
    - By default, old point 'proposals' are treated the same as regular proposals but weighted slightly higher.
        - If `fast_rematch_old_points` if true, if we have proposals for old points, we will entirely skip trying to build better matches using the 2D points matching to old points (so it is much faster but potentially lower quality).
- After all of the above is done, there is a final set of filters to:
    - Merge any points that are very close together (< `min_marker_distance` distance apart)
    - Snap old points to new points if they are very close together (< `max_point_jump` distance apart)

### Rigid Body Tracking

TODO

### Wanding

TODO


## UI

The UI is currently web based (written in HTML/JavaScript) (code is located [here]()). When the host software opens, it spawns a [web view](/pkg/web/view/index.md) using whatever browser runtime is available on the host machine.

Communication with the UI is done by starting a HTTP server on a random port and having the web view send HTTP1 and websocket requests. Note that it is possible to do everything over IPC but some operating systems (GTKWebkit on Linux) have very small IPC connections so HTTP is better and allows standardizing the code base for usecases that are headless and want to use a detached UI accessible from a separate web browser. Also note that HTTP1 has limited concurrency in most browsers so websockets or HTTP2 are needed to have good multiplexing but HTTP2 is only easy to get working if using TLS.