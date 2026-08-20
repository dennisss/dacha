# Optical Motion Capture : Camera Intrinsics Calibration

Before we can combine multiple cameras, we will calibrate each individual camera's intrinsics (this is helpful to deal with high distortion lenses which don't calibrate well without a good initial guess).

## Calibration Checkerboard

You will need a checkerboard calibration board to perform the calibration.

Code is available in [./scripts/generate_checkerboard.py](./scripts/generate_checkerboard.py) to generate a checkerboard PDF that can be printed out in 3mm thick ACM. The settings will need to be modified to match the printing shop you are using.

Alternatively get it made by a specialist vendor like [foamcoreprint.com](https://foamcoreprint.com)

- Outer board size: 16.5" by 24"
- 40mm square size
- 9 x 13 inner corners (10 x 14 squares)
- Get the best 'matte' finish available.
- Get slightly rounded corners if available


## Old Notes

TODO: Rewrite this section.


Then plug the camera into your network and run the following to set it up camera as a cluster node:

```
cargo run --bin cluster_cli -- \
  setup_node \
  --zone=home \
  --node_addr=10.1.1.29 \
  --ssh_args="-i ~/.ssh/id_cluster" \
  --node_config_patch='hardware_timestamped_interfaces: "eth0"' \
  --sysctl_patch='net.ipv4.ip_unprivileged_port_start = 200'
```

Then mark the cluster node as a mocap camera:

```
cargo run --bin cluster_cli --  labels set --node_id=jx31hfj1hmfpd "mocap_camera=yes"
```

TODO: Integrate this into the previous step.


If this is your first camera, run the following to load the camera software across all mocap cameras in the cluster:

```
cargo run --bin cluster_cli -- start_job pkg/vision/mocap/config/camera.job
```

You can monitor all the created nodes and workers (one worker per camera node to run the software) by running these commands:

```
cargo run --bin cluster_cli -- list nodes
cargo run --bin cluster_cli -- list workers
```

Then flash the PPS divider MCU firmware MCU 

```
# Only need to do once for the first camera.
# TODO: Do this automatically in the next command.
make -C pkg/vision/mocap/pps_divider PLATFORM=stm32g031

cargo run --bin mocap_cli -- flash_mcu \
        --camera_addr=jyg2ns8jqdp39.mocap_camera.worker.home.cluster.internal
```


mocap_camera.jyg2ns8jqdp39

Turn the camera off (disconnect from power) and then completely back on.

TODO: Remove the above line once the flashing step correctly resets the MCU.

You can use a command like the following to view logs of the software:

```
cargo run --bin cluster_cli -- log --worker_name=mocap_camera.jyg2ns8jqdp39 --latest_attempt
```

TODO: Sometimes the camera sensor randomly fails probing so maybe have it retry a few times. We can't do anything until that is healthy so we need it to  be more resilient.

You can click on the link in the `list workers` command to get the web UI for the camera.


To collect image frames for camera calibration:

- 20% strobe
- 6000us exposure

Then focus the camera.

Then run:

```
cargo run --bin mocap_cli -- grab_frames \
    --camera_addr=hcgztnjdeqeb8.mocap_camera.worker.home.cluster.internal \
    --output_dir=data/mocap_camera_calib/hcgztnjdeqeb8/
```




