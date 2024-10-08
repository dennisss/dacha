use base_error::*;

/*
; objects_info = {"objects":[{"name":"xyzCalibration_cube.stl (Instance 1)","polygon":[[154.024,151.627],[134.024,151.627],[134.024,131.627],[154.024,131.627]]},{"name":"xyzCalibration_cube.stl (Instance 2)","polygon":[[175.918,230.018],[155.918,230.018],[155.918,210.018],[175.918,210.018]]},{"name":"xyzCalibration_cube.stl (Instance 3)","polygon":[[255.894,150.182],[235.894,150.182],[235.894,130.182],[255.894,130.182]]}]}
*/
#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct ObjectsInfo {
    pub objects: Vec<ObjectInfo>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct ObjectInfo {
    pub name: String,
    pub polygon: Vec<Vec<f32>>,
}
