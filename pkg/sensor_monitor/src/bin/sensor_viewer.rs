#[macro_use]
extern crate common;
extern crate graphics;
extern crate image;
extern crate crypto;
extern crate sensor_monitor;
extern crate rpc;
extern crate math;

use std::rc::Rc;

use common::errors::*;
use image::Color;
use crypto::random::clocked_rng;
use crypto::random::RngExt;
use math::matrix::vec2f;

use graphics::canvas::Paint;
use graphics::font::{CanvasFontRenderer, OpenTypeFont};
use graphics::ui;
use graphics::ui::chart::*;

async fn run() -> Result<()> {
    let font_family = Rc::new(CanvasFontRenderer::new(
        OpenTypeFont::read(project_path!("testdata/noto-sans.ttf")).await?,
    ));

    let x_range = Range {
        min: 0.,
        max: 1000.,
    };
    let y_range = Range { min: 0., max: 10. };

    let mut points = vec![];

    let mut x = 0.;
    let mut y = 5.;

    let mut rng = clocked_rng();
    while x < x_range.max + 10. {
        points.push(vec2f(x, y));
        x += 1.;
        y += 2. * rng.between::<f32>(-0.5, 0.5);
        y = (0.9 * y) + (0.1 * 5.);
    }

    let candidate_intervals = &[
        5. * 60. * 1000.,  // 5 minutes
        10. * 60. * 1000., // 10 minutes
        30. * 60. * 1000., // 30 minutes
        60. * 60. * 1000., // 1 hour
    ];

    let duration = x_range.max - x_range.min;

    let mut interval = candidate_intervals[0];
    for candidate_interval in candidate_intervals {
        if duration / candidate_interval > 5. {
            interval = *candidate_interval;
        } else {
            break;
        }
    }

    let mut x_ticks: Vec<Tick> = vec![];
    {
        let mut current_tick = (x_range.min / interval).floor() * interval;

        /*
        while current_tick < x_range.max {
            let time = new Date(current_tick);
            let label = time.getHours().toString().padStart(2, '0') + ':' + time.getMinutes().toString().padStart(2, '0');

            x_ticks.push({ value: current_tick, label });
            current_tick += interval;
        }
        */
    }

    let y_ticks = [0., 2.5, 5., 7.5, 10.].iter().cloned().map(|value| Tick {
        value,
        label: value.to_string(),
    }).collect::<Vec<_>>();


    /*
        async _make_request() {
        let now = (new Date()).getTime() * 1000;
        let end_timestamp = now;
        let start_timestamp = end_timestamp - (60 * 60 * 1000000);

        const resp = await fetch('/api/query', {
            method: 'POST',
            cache: 'no-cache',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                start_timestamp,
                end_timestamp,
                metric_name: 'random'
            })
        });

        let obj = await resp.json();

        this._x_axis = { min: start_timestamp / 1000, max: end_timestamp / 1000 };

        this._data = [];
        for (var i = 0; i < obj.lines[0].points.length; i++) {
            let x = obj.lines[0].points[i].timestamp * 1;
            if (x < start_timestamp || x > end_timestamp) {
                console.error('Bad point');
            }

            this._data.push({ x: x / 1000, y: obj.lines[0].points[i].value });
        }

        // console.log('Num points: ' + this._data.length);

        // this._data = this._data.slice(0, 100);

        this._draw_frame();

        // TODO: Run this relative to the time at which we started to do the current refresh.
        setTimeout(() => this._make_request(), 2000);
    }

    
    */

    let root_el = ui::Element::from(ChartViewParams {
        options: ChartOptions {
            margin: Margin {
                left: 40.,
                bottom: 20.,
                top: 6.,
                right: 2.,
            },
            grid: Grid {
                line_width: 1.,
                line_color: "#ccc".parse()?,
                label_paint: Paint::color("#000".parse()?),
                x_ticks,
                y_ticks,
            },
            data_line_width: 1.,
            data_line_color: "#4af".parse()?,
            data_point_paint: Paint::color("#4af".parse()?),
            data_point_size: 3.,
            font_family,
            font_size: 14.,
        },
        data: ChartData {
            x_range,
            y_range,
            points,
        }
    });

    ui::render_element(root_el, 800, 1000).await
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}