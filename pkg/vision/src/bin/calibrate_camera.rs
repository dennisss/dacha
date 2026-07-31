/*
Runs camera intrinsics calibration given an input folder containing a set of checkerboard images.

cargo run --bin calibrate_camera --release
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;
use image::format::jpeg::encoder::JPEGEncoder;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use executor::FileHandle;
use file::LocalPathBuf;
use macros::executor_main;
use math::array::Array;
use image::{Colorspace, Image};
use file::project_path;
use vision::*;
use vision::drawing::*;
use math::matrix::{vec2d, vec3d, Matrix3d, Vector3d, Vector2d};
use math::matrix::axis_angle::to_axis_angle;


#[executor_main]
async fn main() -> Result<()> {
    let mut options = CheckerboardDetectionOptions::default();
    options.grid_width = 8;
    options.grid_height = 13;

    let square_size = 0.04; // 40mm

    let mut initial_intrinsics = CameraIntrinsicsModel::from_nominal_params(
        1920,
        1200,
        millis(6.0),
        micros(3.),
    );

    let mut points_3d = generate_checkerboard_grid_3d(options.grid_width, options.grid_height, square_size);

    println!("INITIAL PARAMS: {:?}", initial_intrinsics);


    let input_dir = project_path!("data/mocap_camera_calib/ab21z2zt1gf6w");
    // let input_dir = project_path!("data/mocap_camera_calib/jg30xx5m7wcky_39m");
    // let input_dir = project_path!("data/mocap_data_dir/checkerboard/jg30xx5m7wcky/1784779613163867");

    // data/mocap_camera_calib/jg30xx5m7wcky_39m


    let mut first_image = None;

    let mut all_points_2d = vec![];

    // TODO: parallelize and sort the list for first_image.
    for entry in file::read_dir(&input_dir)? {

        if entry.name().ends_with(".jpg") {
            let path = input_dir.join(entry.name());

            let s = Instant::now();

            let img = Image::<u8>::read(&path).await?;
            let e = Instant::now();
            
            let mut res = detect_checkboard(&img, &options).await;

            let e2 = Instant::now();

            println!("{}: {:?}", path.as_str(), res.points.is_some());

            println!("- {:?} ; {:?}", e - s, e2 - e);

            let points_2d = match res.points.take() {
                Some(v) => v,
                None => {
                    println!("=> No match!");
                    continue
                }
            };

            // if first_image.is_none()
            if entry.name().contains("0003") {
                first_image = Some((img.clone(), points_2d.clone(), all_points_2d.len()));
            }


            // for p in points_2d.iter() {
            //     println!("vec2d({:?},{:?}),", p.x(), p.y());
            // }

            all_points_2d.push(points_2d);
        }
    }

    let (input_image, points_2d, input_idx) = first_image.unwrap();


    for num_iters in 1..100 {
        println!("iters: {}", num_iters);

        let mut solver = CameraInstrinsicsSolver::new(&initial_intrinsics);

        solver.set_max_iterations(num_iters);

        for points_2d in &all_points_2d {
            solver.add_object(
                &points_3d,
                points_2d
            );
        }

        let res = solver.solve();

        // println!("{:#?}", res);

        let mut debug_image = gray_to_color(&input_image);

        let output_dir = project_path!("data/checkerboard_solving");

        {
            for p in &points_2d {
                let p = p.cast();
                draw_big_color_circle(&p, &image::Color::rgb(0, 255, 0), &mut debug_image);
            }

            for p in &res.projected_points[input_idx] {
                let p = p.cast();
                draw_big_color_circle(&p, &image::Color::rgb(255, 0, 0), &mut debug_image);
            }

        }

        {
            let p = output_dir.join(format!("{:04}.jpg", num_iters));

            let encoder = JPEGEncoder::new(100);
            let mut data = vec![];
            encoder.encode(&debug_image, &mut data)?;

            file::write(p, &data).await?;
        }

        println!("ERROR: {}", res.error);
    }




    /*
    let output_dir = project_path!("data/checkerboard_debug");

    let img = Image::<u8>::read(&input_path).await?;

    let mut options = CheckerboardDetectionOptions::default();
    options.grid_width = 8;
    options.grid_height = 13;

    /*
    8 x 13 (40mm spacing)
    */
    let mut res = detect_checkboard(&img, &options).await;

    for (i, img) in res.debug_images.into_iter().enumerate() {


    }
    */

    return Ok(());


    Ok(())
}