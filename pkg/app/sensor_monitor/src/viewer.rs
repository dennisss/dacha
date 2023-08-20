use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::errors::*;
use image::Color;

use common::chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use crypto::random::clocked_rng;
use crypto::random::RngExt;
use graphics::canvas::Paint;
use graphics::font::CanvasFontRenderer;
use graphics::ui;
use graphics::ui::chart::*;
use graphics::ui::grid::{GridDimensionSize, GridViewParams};
use graphics::ui::virtual_view::*;
use math::matrix::vec2;

use crate::proto::data::*;

#[derive(Clone)]
pub struct MetricViewer {
    pub font_family: Rc<CanvasFontRenderer>,
    pub metric_stub: MetricStub,
}

pub struct MetricViewerView {
    params: MetricViewer,
    state: Arc<Mutex<MetricViewerState>>,
    chart_options: ChartOptions,
}

struct MetricViewerState {
    chart_data: ChartData,
    dirty: bool,
}

struct MetricViewerItem {
    name: String,
    price: f32,
}

impl VirtualViewParams for MetricViewer {
    type View = MetricViewerView;
}

impl MetricViewerView {
    fn get_random_data() -> ChartData {
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
            points.push(vec2(x, y));
            x += 1.;
            y += 2. * rng.between::<f64>(-0.5, 0.5);
            y = (0.9 * y) + (0.1 * 5.);
        }

        ChartData {
            points,
            x_range,
            y_range,
        }
    }

    fn get_options_for_data(params: &MetricViewer, data: &ChartData) -> Result<ChartOptions> {
        let candidate_intervals = &[
            5. * 60. * 1000.,  // 5 minutes
            10. * 60. * 1000., // 10 minutes
            30. * 60. * 1000., // 30 minutes
            60. * 60. * 1000., // 1 hour
        ];

        let duration = data.x_range.max - data.x_range.min;

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
            // Interprate values as milliseconds since epoch values.

            let mut current_tick = (data.x_range.min / interval).floor() * interval;

            while current_tick < data.x_range.max {
                let time = Local.from_utc_datetime(&NaiveDateTime::from_timestamp(
                    (current_tick as i64) / 1000,
                    0,
                ));

                let label = time.format("%H:%M").to_string();
                x_ticks.push(Tick {
                    value: current_tick,
                    label,
                });

                current_tick += interval;
            }
        }

        let y_ticks = [0., 2.5, 5., 7.5, 10.]
            .iter()
            .cloned()
            .map(|value| Tick {
                value,
                label: value.to_string(),
            })
            .collect::<Vec<_>>();

        Ok(ChartOptions {
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
            font_family: params.font_family.clone(),
            font_size: 14.,
        })
    }

    async fn background_thread(
        stub: MetricStub,
        state: Arc<Mutex<MetricViewerState>>,
    ) -> Result<()> {
        let ctx = rpc::ClientRequestContext::default();

        loop {
            let now = (Utc::now().timestamp_millis() as u64) * 1000;

            let mut request = QueryRequest::default();
            request.set_start_timestamp(now - (60 * 60 * 1000 * 1000)); // 1 hour back.
            request.set_end_timestamp(now);
            request.set_metric_name("random");

            let response = stub.Query(&ctx, &request).await;
            let obj = match response.result {
                Ok(v) => v,
                Err(e) => {
                    println!("request failed: {}", e);
                    executor::sleep(Duration::from_secs(2)).await?;
                    continue;
                }
            };

            let y_range = Range { min: 0., max: 10. };
            let x_range = Range {
                min: (request.start_timestamp() / 1000) as f64,
                max: (request.end_timestamp() / 1000) as f64,
            };

            let mut points = vec![];

            for p in obj.lines()[0].points() {
                let x = (p.timestamp() / 1000) as f64;
                assert!(
                    p.timestamp() >= request.start_timestamp()
                        && p.timestamp() <= request.end_timestamp()
                );
                points.push(vec2(x, p.value() as f64));
            }

            // for i in 0..10 {
            //     points.push(vec2( x_range.min + ((i as f64) / 10.) * (x_range.max -
            // x_range.min), i as f64)); }

            let chart_data = ChartData {
                points,
                x_range,
                y_range,
            };

            {
                let mut state = state.lock().unwrap();
                state.chart_data = chart_data;
                state.dirty = true;
            }

            // TODO: Run this relative to the time at which we started to do the current
            // refresh.
            executor::sleep(Duration::from_secs(2)).await?;
        }
    }
}

impl VirtualView for MetricViewerView {
    type Params = MetricViewer;

    fn create_with_params(params: &Self::Params) -> Result<Self> {
        let chart_data = Self::get_random_data();
        let chart_options = Self::get_options_for_data(params, &chart_data)?;

        let state = Arc::new(Mutex::new(MetricViewerState {
            chart_data,
            dirty: false,
        }));

        executor::spawn({
            let stub = params.metric_stub.clone();
            let state = state.clone();
            async move {
                let e = Self::background_thread(stub, state).await;
                if let Err(e) = e {
                    println!("background thread failed: {}", e);
                }
            }
        });

        Ok(Self {
            params: params.clone(),
            chart_options,
            state,
        })
    }

    fn update_with_params(&mut self, params: &Self::Params) -> Result<()> {
        self.params = params.clone();
        Ok(())
    }

    fn build_element(&mut self) -> Result<ui::Element> {
        let mut state = self.state.lock().unwrap();

        if state.dirty {
            self.chart_options = Self::get_options_for_data(&self.params, &state.chart_data)?;
            state.dirty = false;
        }

        Ok(ui::Element::from(ChartViewParams {
            options: self.chart_options.clone(),
            data: state.chart_data.clone(),
        }))
    }
}
