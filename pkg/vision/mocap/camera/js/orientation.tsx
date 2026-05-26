

// TODO: Generalize to sideways cameras.
// TODO: Also add this to the per-camera UI.
export function camera_orientation(camera_status) {

    let accel = camera_status.accelerometer.value;

    let upside_down = accel.y > 0 ? true : false;

    let horizon_angle = Math.round((180 / Math.PI) * Math.atan(accel.z / Math.abs(accel.y)));

    return {
        upside_down,
        horizon_angle
    };
}