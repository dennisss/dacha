
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::f32::consts::PI;

use common::errors::*;
use math::matrix::{VectorXd, MatrixXd};
use math::vecxd;
use math::matrix::qr::QR;
use math::matrix::pinv;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cnc_controller_proto::cnc::*;
use cnc_controller::config::ControllerConfigRegistry;
use math_proto_util::*;
use math_proto::math::*;
use file::LocalPathBuf;
use media_camera::camera_manager::CameraManager;
use math_compute::io::CSVDataReader;
use file::project_path;
use cnc_controller::stats::MinMaxStats;
use cnc::grid::*;

use crate::remote::*;


#[derive(Args)]
pub struct SkewCalibrationCommand {
    mode: Mode
}

#[derive(Args)]
enum Mode {
    #[arg(name = "scan")]
    Scan,

    #[arg(name = "dump-video")]
    DumpVideo,

    #[arg(name = "calculate")]
    Calculate(CalculateMode),
}

#[derive(Args)]
struct DumpVideoMode {

    /// Where to save the final skew matrix.
    input_path: LocalPathBuf,
}


#[derive(Args)]
struct CalculateMode {

    /// Where to save the final skew matrix.
    output_path: LocalPathBuf,
}

impl SkewCalibrationCommand {
    pub async fn run(self) -> Result<()> {
        
        match self.mode {
            Mode::Scan => Self::run_scan().await?,
            Mode::DumpVideo => Self::run_dump_video().await?,
            Mode::Calculate(mode) => Self::run_calculate(mode).await?
        }

        Ok(())
    }

    async fn run_scan() -> Result<()> {

        let mut machine = RemoteMachineController::create().await?;

        let camera_manager = CameraManager::create()?;

        let camera_entry = {
            let mut camera_entries = camera_manager.list().await?;
            if camera_entries.len() != 1 {
                return Err(err_msg("Expected only a single camera to be attached"));
            }

            camera_entries.into_values().next().unwrap()
        };

        println!("Using camera: {}", camera_entry.id());
        let mut camera_subscriber = camera_manager.open(camera_entry).await?;
        // let format = camera_subscriber.format().await?;
        // TODO: Verify format is MJPG of highest quality.

        let start_time = Instant::now();

        let mut out = SkewData::default();

        file::create_dir_all(file::project_dir().join("data/skew")).await?;

        let feed_rate = 40.0;

        let grid = Self::get_scan_grid();

        let mut i = 0;

        for round in 1..7 {
            println!("### Round: {}", round);
            machine.move_to(&vecxd!(60.0, 60.0, 80.0), feed_rate).await?;
            machine.wait_until_idle().await?;

            println!("Continue: [y/N]?");
            if !file::read_user_confirmation().await? {
                return Ok(());
            }

            for (x, y, z) in grid.clone() {
                let mut pos = vecxd!(x, y, z);
                if round > 2 {
                    pos[2] += 15.0;
                }

                machine.move_to(&pos, feed_rate).await?;
                machine.wait_until_idle().await?;

                let frame = executor::timeout(Duration::from_millis(5000), camera_subscriber.recv_new()).await??;
                let data = frame.data.data().unwrap();

                file::write(file::project_dir().join("data/skew").join(format!("{:04}.jpg", i)), data).await?;

                {
                    let pt = out.new_points();
                    if round <= 2 {
                        pt.set_fixed_group(round as u32);
                    }
                    pt.set_machine_position(pos.to_proto());
                    pt.set_image_index(i as u32);
                    pt.set_time_micros((Instant::now() - start_time).as_micros() as u64);
                }

                i += 1;
            }
        }

        file::write("data/skew/data.txtpb", protobuf::text::serialize_text_proto(&out).as_bytes()).await?;

        Ok(())
    }

    fn get_scan_grid() -> Vec<(f64, f64, f64)> {
        let mut out = vec![];

        let mesh = Grid::create((5., 5.), (115., 115.), 8, 8).scan_order();
        // let z_offsets = [20., 30., 40.0];
        let z_offsets = [30., 40., 50.0];

        for (z_i, z) in z_offsets.iter().cloned().enumerate() {

            let mut mesh = mesh.clone();
            if z_i % 2 == 1 {
                mesh.reverse();
            }

            for (x, y) in mesh {
                out.push((x, y, z));
            }
        }

        out
    }

    async fn run_dump_video() -> Result<()> {
        let mut data = SkewData::default();
        {
            let s = file::read_to_string("data/skew/data.txtpb").await?;
            protobuf::text::parse_text_proto(&s, &mut data)?;
        }

        let mut out = String::new();

        for (pt_i, pt) in data.points().iter().enumerate() {
            let len = {
                if pt_i == data.points().len() - 1 {
                    Duration::from_secs(2)
                } else {
                    Duration::from_micros(data.points()[pt_i + 1].time_micros() - pt.time_micros())
                }
            };

            let file_line = format!("file 'data/skew_out/debug_{:04}.jpg'\n", pt.image_index());

            out.push_str(&file_line);
            out.push_str(&format!("duration {}\n", len.as_secs_f32()));

            // See https://gemini.google.com/app/c1820cca3230e635
            if pt_i == data.points().len() - 1 {
                out.push_str(&file_line);
            }
        }

        file::write("skew_video_stamps.txt", out).await?;

        Ok(())
    }


    /*
    I want to solve for 'machine_position = Mat * real_pos'
    [3xn] = [3x4] [4xn]
    */

    async fn read_csv(csv: &mut CSVDataReader, out: &mut MatrixXd) -> Result<()> {
        let mut i = 0;
        while let Some(row) = csv.read().await? {
            let cam_x = row.f32_field("Cam_X_mm")?;
            let cam_y = row.f32_field("Cam_Y_mm")?;
            let cam_z = row.f32_field("Cam_Z_mm")?;

            out[(0, i)] = cam_x as f64;
            out[(1, i)] = cam_y as f64;
            out[(2, i)] = cam_z as f64;
            out[(3, i)] = 1.0;

            i += 1;

            if i == out.cols() {
                break;
            }
        }

        Ok(())
    }

    fn calculate_skew_mat(machine_pos: &MatrixXd, real_pos: &MatrixXd) -> Result<MatrixXd> {
        let real_pos_pinv = pinv(real_pos);

        let t = machine_pos * real_pos_pinv;

        {
            let mut stats = MinMaxStats::default();

            let machine_pos_pred = &t * real_pos; 
            for i in 0..machine_pos_pred.cols() {
                let error: MatrixXd = machine_pos_pred.block_with_shape(0, i, 3, 1) - machine_pos.block_with_shape(0, i, 3, 1);
                stats.add(error.norm());
            }

            println!("Error (mm): {}", stats.print());
        }

        println!("Raw: {:?}", t);

        let t_3x3: MatrixXd = t.block_with_shape(0, 0, 3, 3).to_owned();

        let t_clean = rq_decomposition(&t_3x3);
        println!("Transform (Clean):\n{:?}", t_clean);

        Ok(t_clean)

    }


    async fn run_calculate(mode: CalculateMode) -> Result<()> {
        let mut csv = CSVDataReader::create(&project_path!("camera_poses.csv")).await?;

        let machine_scan_grid = Self::get_scan_grid();
        let n = machine_scan_grid.len();

        let mut machine_pos = MatrixXd::zero_with_shape(3, n);
        for (i, (x,y,z)) in machine_scan_grid.into_iter().enumerate() {
            machine_pos[(0, i)] = x as f64;
            machine_pos[(1, i)] = y as f64;
            machine_pos[(2, i)] = z as f64;
            // machine_pos[(3, i)] = 1.0;
        }

        let mut real_pos1 = MatrixXd::zero_with_shape(4, n);
        Self::read_csv(&mut csv, &mut real_pos1).await?;


        // Just for visualization.
        {
            println!("=====");
            for i in 0..n {
                println!("{{ x: {}, y: {}, z: {} }},", machine_pos[(0, i)], machine_pos[(1, i)], machine_pos[(2, i)]);
            }

            let real_pos_pinv = pinv(&real_pos1);
            let t = &machine_pos * real_pos_pinv;

            let aligned_real_pos = t * &real_pos1;

            println!("=====");
            for i in 0..n {
                println!("{{ x: {}, y: {}, z: {} }},", aligned_real_pos[(0, i)], aligned_real_pos[(1, i)], aligned_real_pos[(2, i)]);
            }

            /*
            println!("=====");
            for i in 0..n {
                println!("{{ x: {}, y: {}, z: {} }},", real_pos1[(0, i)], real_pos1[(1, i)], real_pos1[(2, i)]);
            }
            */


            return Ok(());
        }

        let mut real_pos2 = MatrixXd::zero_with_shape(4, n);
        Self::read_csv(&mut csv, &mut real_pos2).await?;


        let skew1 = Self::calculate_skew_mat(&machine_pos, &real_pos1)?;
        let skew2 = Self::calculate_skew_mat(&machine_pos, &real_pos2)?;

        let mut avg_skew = &skew1 + &skew2;
        avg_skew /= 2.0;

        println!("Final:\n{:?}", avg_skew);


        /*
        // Divide rows by the diagonal entries.
        for i in 0..3 {
            let scale = avg_skew[(i, i)];
            for j in 0..3 {
                avg_skew[(i,j)] /= scale;
            }
        }
        println!("Final (No Scaling):\n{:?}", avg_skew);
        */

        let test_point = MatrixXd::from_slice_with_shape(3, 1, &[ 100., 100., 0. ]);
        println!("Transformed Point: {:?}", &avg_skew * test_point);


        file::write(mode.output_path, protobuf::text::serialize_text_proto(
            &avg_skew.to_proto()).as_bytes()).await?;

        Ok(())
    }


}

use math::matrix::Dynamic;

/*
https://leohart.wordpress.com/2010/07/23/rq-decomposition-from-qr-decomposition/
*/
fn rq_decomposition(x: &MatrixXd) -> MatrixXd {

    // Reverse rows.
    let mut x_flipped = x.clone();
    for i in 0..x.rows() {
        for j in 0..x.cols() {
            x_flipped[(i, j)] = x[(x.rows() - 1 - i, j)];
        }

        // x_flipped.block_with_shape_mut::<Dynamic, Dynamic>(i, 0, 1, x.cols()).copy_from(
        //     &x.block_with_shape(x.rows() - 1 - i, 0, 1, x.cols()));
    }

    let qr_tmp = QR::householder(&x_flipped.transpose());

    let r_tmp = qr_tmp.r.transpose();

    let mut r = r_tmp.clone();
    for i in 0..x.rows() {
        for j in 0..x.cols() {
            r[(i, j)] = r_tmp[(x.rows() - 1 - i, x.cols() - 1 - j)];
        }
    }

    let mut r_normalized = r.clone();

    for i in 0..x.rows() {
        if r_normalized[(i, i)] < 0.0 {
            for j in 0..x.rows() {
                r_normalized[(j, i)] *= -1.0;
            }
        }
    }

    r_normalized

}


