

// TODO: Generalize to sideways cameras.
// TODO: Also add this to the per-camera UI.
export function camera_orientation(camera_status) {

    let accel = camera_status.accelerometer.value;

    let y = accel.values[1];
    let z = accel.values[2];

    let upside_down = y > 0 ? true : false;

    let horizon_angle = Math.round((180 / Math.PI) * Math.atan(z / Math.abs(y)));

    return {
        upside_down,
        horizon_angle
    };
}