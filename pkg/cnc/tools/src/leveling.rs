/*
cargo run --bin cnc_tools -- leveling probe-variance

cargo run --bin cnc_tools -- leveling mesh-level --output_path=mesh.txtpb

cargo run --bin cnc_tools -- leveling dump-mesh --input_path=mesh_pretrim.txtpb


*/


use std::sync::Arc;
use std::time::Duration;
use std::f64::consts::PI;

use common::errors::*;
use math::matrix::{VectorXd, MatrixXd};
use math::vecxd;
use math::matrix::svd::SVD;
use math::matrix::qr::QR;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cluster_client::ClusterServer;
use cnc_controller_proto::cnc::*;
use cnc_controller::config::ControllerConfigRegistry;
use file::LocalPathBuf;
use media_camera::camera_manager::CameraManager;
use cnc_controller::csv::CSVDataReader;
use file::project_path;
use cnc_controller::stats::*;
use cnc::grid::*;

use crate::remote::*;
use crate::plane::*;

#[derive(Args)]
pub struct LevelingCommand {
    mode: Mode
}

#[derive(Args)]
enum Mode {
    #[arg(name = "probe-variance")]
    ProbeVariance,

    #[arg(name = "mesh-level")]
    MeshLevel(MeshMode),

    #[arg(name = "dump-mesh")]
    DumpMesh(DumpMeshMode),

    #[arg(name = "wipe-nozzle")]
    WipeNozzle
}

#[derive(Args)]
struct MeshMode {
    #[arg(default = 8)]
    size: usize,
    output_path: LocalPathBuf
}

#[derive(Args)]
struct DumpMeshMode {
    input_path: LocalPathBuf
}


impl LevelingCommand {
    pub async fn run(self) -> Result<()> {
        
        match self.mode {
            Mode::ProbeVariance => Self::run_probe_variance().await?,
            Mode::MeshLevel(mode) => Self::run_mesh_level(mode).await?,
            Mode::DumpMesh(mode) => Self::run_dump_mesh(mode).await?,
            Mode::WipeNozzle => Self::run_wipe_nozzle().await?,
        }

        Ok(())
    }

    async fn run_probe_variance() -> Result<()> {
        let mut machine = RemoteMachineController::create().await?;

        let travel_feed_rate_xy = 40.0;
        let travel_position_z = 10.0;
        let probe_feed_rate = 10.0;
        let travel_feed_rate_z = 10.0;

        let mut current_pos = machine.last_position().await?;
        
        println!("Start pos: {:?}", current_pos);

        // Move to center of bed
        current_pos[0] = 60.0;
        current_pos[1] = 60.0;
        current_pos[2] = travel_position_z;
        machine.move_to(&current_pos, travel_feed_rate_xy).await?;

        let mut variance = vec![];
        let mut variance_stats = MinMaxStats::default();

        let mut hit_variance = vec![];
        let mut hit_variance_stats = MinMaxStats::default();
        for i in 0..100 {
            println!("Probe: {}", i);

            machine.wait_until_idle().await?;
            executor::sleep(Duration::from_millis(1000)).await?;

            // TODO: Need the Z stall probe available as a backstop incase there is a probing issue.
            // (and will thus need some guarantee that we can stop the endstop controller and get a consistent snapshot of all endstops triggered at a point in time).
            current_pos[2] = -5.0;
            let hit_position = machine.move_towards_endstop(&current_pos, probe_feed_rate).await?;

            // I need to wait for all the motors to resync.
            executor::sleep(Duration::from_millis(100)).await?;

            let pos = machine.last_position().await?;
            
            // TODO: Make this filtering more consistent and also do retries if we are doing stuff like homing.
            if pos.z().abs() > 2.0 {
                eprintln!("Rejecting point at: {}", pos.z().abs());
            } else {
                variance.push(pos.z());
                variance_stats.add(pos.z());

                println!("=> {}", pos.z());
                if let Some(pos) = hit_position {
                    println!("=> (hit z: {})", pos.z());
                    hit_variance.push(pos.z());
                    hit_variance_stats.add(pos.z());
                }
            }

            // Lift
            current_pos[2] = travel_position_z;
            machine.move_to(&current_pos, travel_feed_rate_z).await?;
        }

        println!("Z Values {:?}", variance);
        println!("Std Dev: {}", compute_standard_deviation(&variance));
        println!("{}", variance_stats.print());

        println!("");

        println!("Hit Z Values {:?}", hit_variance);
        println!("Std Dev: {}", compute_standard_deviation(&hit_variance));

        println!("{}", hit_variance_stats.print());

        Ok(())
    }

    async fn run_mesh_level(mode: MeshMode) -> Result<()> {
        let mut machine = RemoteMachineController::create().await?;

        let leveler = ZGridFadeLeveler::probe(&mut machine, mode.size).await?;

        file::write(mode.output_path, protobuf::text::serialize_text_proto(
            &leveler.to_proto()).as_bytes()).await?;

        // println!("Mesh: {:?}", mesh_points);

        Ok(())
    }

    async fn run_dump_mesh(mode: DumpMeshMode) -> Result<()> {
        let mut proto = ZGridFadeLevelerProto::default();
        let data = file::read_to_string(&mode.input_path).await?;
        protobuf::text::parse_text_proto(&data, &mut proto)?;
        
        let leveler = ZGridFadeLeveler::from_proto(&proto);

        let mut pts = vec![];
        for (x,y,z) in leveler.z_values.iter() {
            pts.push(vecxd!(x, y, z));
            println!("{{ x: {}, y: {}, z: {} }},", x, y, z);
        }

        // https://github.com/VoronDesign/Voron-0/blob/Voron0.2r1/Drawings/Buildplate_v0.2.PDF
        let holes = vec![
            vecxd!(5.0, 115.0), // Top-left
            vecxd!(115.0, 115.0), // Top right
            vecxd!(60.0, 5.0), // Bottom
        ];


        let mut plane = Plane::fit_near_flat(&pts).unwrap();

        plane.c = 0.0;

        println!("Plane: {:?}", plane);

        /*
        +Z means the point is too low (must raise)
        -Z means the point is too high (must lower)
        */
        for pt in &holes {
            let z = plane.compute_z(pt.x(), pt.y());
            println!("Hole Z: {}", z);
        }

        Ok(())
    }

    async fn run_wipe_nozzle() -> Result<()> {

        let travel_feed_rate_xy = 40.;
        let wipe_feed_rate_xy = 20.;
        let travel_feed_rate_z = 10.;

        let mut machine = RemoteMachineController::create().await?;

        let mut current_pos = machine.last_position().await?;

        current_pos[0] = 60.;
        current_pos[1] = 60.;
        // Must be low enough to clear the wiper
        current_pos[2] = 20.;
        machine.move_to(&current_pos, travel_feed_rate_z).await?;
        machine.wait_until_idle().await?;

        // Extended
        machine.set_servo_position(1750.).await?;
        executor::sleep(Duration::from_millis(200)).await?;

        current_pos[1] = 111.;
        machine.move_to(&current_pos, travel_feed_rate_xy).await?;

        current_pos[0] = 30.;
        machine.move_to(&current_pos, travel_feed_rate_xy).await?;

        current_pos[0] = 2.;
        machine.move_to(&current_pos, wipe_feed_rate_xy).await?;

        current_pos[1] = 108.;
        machine.move_to(&current_pos, wipe_feed_rate_xy).await?;

        current_pos[0] = 30.;
        machine.move_to(&current_pos, wipe_feed_rate_xy).await?;

        machine.wait_until_idle().await?;

        // Closed (no need to wait for it to close)
        machine.set_servo_position(850.).await?;

        current_pos[0] = 60.;
        machine.move_to(&current_pos, travel_feed_rate_xy).await?;

        current_pos[1] = 60.;
        machine.move_to(&current_pos, travel_feed_rate_xy).await?;

        Ok(())
    }
}

const MIN_MOVE_SIZE: f64 = 0.1;
const FADE_HEIGHT: f64 = 20.0;
const MAX_ERROR: f64 = 0.02;

// TODO: Base this on a fraction of the grid size?
const STEP_SIZE: f64 = 5.0;


pub struct ZGridFadeLeveler {
    z_values: GridValues,
    plane: Plane,
}

impl ZGridFadeLeveler {

    pub async fn probe(machine: &mut RemoteMachineController, size: usize) -> Result<Self> {
        let travel_feed_rate_xy = 40.0;
        let travel_position_z = 2.0;
        let probe_feed_rate = 10.0;
        let travel_feed_rate_z = 10.0;

        let grid = Grid::create((5., 5.), (115., 115.), size, size);
        
        let grid_pts = grid.scan_order();

        let mut mesh_points = vec![];

        let mut current_pos = machine.last_position().await?;

        /*
        - Drop every point twice.
            - If points don't agree within a threshold, sample a third time.
            - 
        */

        // TODO: This needs to have new features like retrying probes that aren't consistent.
        for (x, y) in grid_pts {
            current_pos[0] = x;
            current_pos[1] = y;
            // TODO: also move to travel Z position.

            machine.move_to(&current_pos, travel_feed_rate_xy).await?;
            machine.wait_until_idle().await?;
            // TODO: Maybe wait for all vibrations to settle.

            let mut z = 0.0;

            loop {
                current_pos[2] = -5.0;
                let hit_pos = machine.move_towards_endstop(&current_pos, probe_feed_rate).await?
                    .unwrap();

                // Lift
                current_pos[2] = travel_position_z;
                machine.move_to(&current_pos, travel_feed_rate_z).await?;

                let pos = hit_pos.z();
                println!("Z: {}", pos);
                if pos.abs() < 0.5 {
                    z = pos;
                    break;
                }

                eprintln!("=> Reject!");
            }

            // tries.sort_by(|a, b| a.partial_cmp(b).unwrap());

            // TODO: Always pick the middle one.
            mesh_points.push(z);


        }

        current_pos[0] = 60.0;
        current_pos[1] = 60.0;
        machine.move_to(&current_pos, travel_feed_rate_xy).await?;

        Ok(Self::create(GridValues::from_scan_values(grid, &mesh_points)?))
    }

    pub fn to_proto(&self) -> ZGridFadeLevelerProto {
        let mut out = ZGridFadeLevelerProto::default();
        out.set_z_values(self.z_values.to_proto());
        out
    }

    pub fn from_proto(proto: &ZGridFadeLevelerProto) -> Self {
        Self::create(GridValues::from_proto(proto.z_values()))
    } 

    fn create(z_values: GridValues) -> Self {
        let mut pts = vec![];
        for (x,y,z) in z_values.iter() {
            pts.push(vecxd!(x, y, z));
        }

        let plane = Plane::fit_near_flat(&pts).unwrap();
        Self {
            z_values,
            plane
        }
    }

    fn new_z(&self, pos: &VectorXd) -> f64 {
        // TODO: Blend these two values.
        let zero_z = self.z_values.interpolate_value(pos.x(), pos.y());
        let zero_z2 = self.plane.compute_z(pos.x(), pos.y());

        let zero_z = {
            zero_z * 0.8 + zero_z2 * 0.2
        };

        // TODO: Should ideally always offset based on average plane?
        return zero_z + pos.z();

        // TODO: Bring back this fade leveling.

        if pos.z() >= FADE_HEIGHT {
            return pos.z();
        }



        if pos.z() <= 1.0 {
            return zero_z + pos.z();
        }

        let fade_percent = (pos.z() - 1.0) / (FADE_HEIGHT - 1.0);

        let offset = (1.0 - fade_percent) * zero_z;

        offset + pos.z()
    }

    /// Returns the list of all points to visit AFTER start_position.
    /// (will just be a list with 1 element containing just end_position is
    ///  no Z compensation or very little is needed)
    pub fn rewrite_move(
        &self,
        start_position: &VectorXd,
        end_position: &VectorXd,
        rapid: bool
    ) -> Vec<VectorXd> {

        cnc::rewriting::rewrite_move_z(
            start_position,
            end_position,
            rapid,
            |pos| self.new_z(pos),
            &cnc::rewriting::RewriteMoveOptions {
                min_move_size: MIN_MOVE_SIZE,
                max_error: MAX_ERROR,
                step_size: STEP_SIZE
            }
        )
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_fade_test() {
        let grid = Grid::create((0., 0.), (10., 10.), 3, 3);

        let grid_values = GridValues::from_scan_values(grid.clone(), &[
            0.0, -1.0, 0.0,
            0.0, -1.0, 0.0,
            0.0, -1.0, 0.0,
        ]).unwrap();

        let leveler = ZGridFadeLeveler {
            z_values: grid_values,
            plane: todo!()
        };

        assert_eq!(
            leveler.rewrite_move(&vecxd!(0.0, 0.0, 0.0), &vecxd!(10.0, 0.0, 0.0), false),
            vec![vecxd!(5.0, 0.0, -1.0), vecxd!(10.0, 0.0, 0.0)]
        );

        assert_eq!(
            leveler.rewrite_move(&vecxd!(0.0, 0.0, 0.0), &vecxd!(20.0, 0.0, 0.0), false),
            vec![vecxd!(5.0, 0.0, -1.0), vecxd!(10.0, 0.0, 0.0), vecxd!(20.0, 0.0, 0.0)]
        );

        assert_eq!(
            leveler.rewrite_move(&vecxd!(0.0, 0.0, 5.0), &vecxd!(20.0, 0.0, 5.0), false),
            vec![vecxd!(5.0, 0.0, 4.5), vecxd!(10.0, 0.0, 5.0), vecxd!(20.0, 0.0, 5.0)]
        );

        assert_eq!(
            leveler.rewrite_move(&vecxd!(0.0, 0.0, 10.0), &vecxd!(20.0, 0.0, 10.0), false),
            vec![vecxd!(20.0, 0.0, 10.0)]
        );
        assert_eq!(
            leveler.rewrite_move(&vecxd!(0.0, 0.0, 15.0), &vecxd!(20.0, 0.0, 15.0), false),
            vec![vecxd!(20.0, 0.0, 15.0)]
        );

        assert_eq!(
            leveler.rewrite_move(&vecxd!(0.0, 0.0, 15.0), &vecxd!(20.0, 0.0, 20.0), false),
            vec![vecxd!(20.0, 0.0, 20.0)]
        );
    }

}


