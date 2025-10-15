use chrono::NaiveDateTime;
use clap::Parser;
use csv::ReaderBuilder;
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Render a porkchop heatmap from CSV (dv_total or c3)"
)]
struct Cli {
    #[arg(long)]
    input: String,
    #[arg(long, default_value = "artifacts/pork.png")]
    output: PathBuf,
    #[arg(long, default_value = "dv_total_km_s")]
    metric: String,
    #[arg(long, default_value_t = 1200)]
    width: u32,
    #[arg(long, default_value_t = 900)]
    height: u32,
    #[arg(long, default_value_t = 4.0)]
    high_clip_factor: f64,
}

#[derive(Debug, Clone)]
struct Cell {
    depart_et: f64,
    arrive_et: f64,
    metric_value: f64,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let metric_request = cli.metric.clone();
    let (cells, mut dep_vals, mut arr_vals, metric_column) =
        read_cells(&cli.input, &metric_request)?;

    if cells.is_empty() {
        return Err(anyhow::anyhow!(
            "No feasible Lambert solutions in the provided CSV"
        ));
    }

    dep_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    dep_vals.dedup();
    arr_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    arr_vals.dedup();

    if dep_vals.is_empty() || arr_vals.is_empty() {
        return Err(anyhow::anyhow!(
            "No feasible Lambert solutions in the provided CSV"
        ));
    }

    if let Some(parent) = cli.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let output_str = cli
        .output
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Output path contains invalid UTF-8"))?;
    let root = BitMapBackend::new(output_str, (cli.width, cli.height)).into_drawing_area();
    root.fill(&WHITE)?;

    let et_depart_min = *dep_vals.first().expect("depart range");
    let et_depart_max = *dep_vals.last().expect("depart range");
    let et_arrive_min = *arr_vals.first().expect("arrive range");
    let et_arrive_max = *arr_vals.last().expect("arrive range");

    let depart_span_days = (et_depart_max - et_depart_min) / 86_400.0;
    let arrive_span_days = (et_arrive_max - et_arrive_min) / 86_400.0;

    let font_family = select_font_family();
    let caption_font = FontDesc::new(font_family, 24.0, FontStyle::Bold);
    let label_font = FontDesc::new(font_family, 18.0, FontStyle::Normal);

    let legend_width = 140i32;
    let (plot_area, legend_area) =
        root.split_horizontally((cli.width as i32 - legend_width).max(200));

    let dep_coords: Vec<f64> = dep_vals
        .iter()
        .map(|et| (et - et_depart_min) / 86_400.0)
        .collect();
    let arr_coords: Vec<f64> = arr_vals
        .iter()
        .map(|et| (et - et_arrive_min) / 86_400.0)
        .collect();

    let grid = build_grid(&cells, &dep_vals, &arr_vals);
    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;
    let mut min_pos: Option<(usize, usize)> = None;
    for (arr_idx, row) in grid.iter().enumerate() {
        for (dep_idx, &v) in row.iter().enumerate() {
            if v.is_finite() {
                if v < min_value {
                    min_value = v;
                    min_pos = Some((dep_idx, arr_idx));
                }
                if v > max_value {
                    max_value = v;
                }
            }
        }
    }

    let (min_dep_idx, min_arr_idx) =
        min_pos.ok_or_else(|| anyhow::anyhow!("No feasible entries in the provided CSV"))?;
    if !max_value.is_finite() {
        max_value = min_value;
    }
    let mut high_clip = (min_value * cli.high_clip_factor).min(max_value);
    if !high_clip.is_finite() || high_clip <= min_value {
        high_clip = max_value.max(min_value * 1.001);
    }

    let grid_clamped: Vec<Vec<f64>> = grid
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| {
                    if v.is_finite() {
                        v.clamp(min_value, high_clip)
                    } else {
                        f64::NAN
                    }
                })
                .collect()
        })
        .collect();

    let levels: Vec<f64> = (0..30)
        .map(|i| {
            let t = i as f64 / 29.0;
            min_value + t * (high_clip - min_value)
        })
        .collect();

    {
        let mut chart = ChartBuilder::on(&plot_area)
            .margin(20)
            .caption("Porkchop heatmap".to_string(), caption_font)
            .x_label_area_size(60)
            .y_label_area_size(90)
            .build_cartesian_2d(0.0..depart_span_days, 0.0..arrive_span_days)?;

        chart
            .configure_mesh()
            .x_desc("Departure Date")
            .y_desc("Arrival Date")
            .label_style(label_font.clone())
            .x_labels(6)
            .y_labels(6)
            .x_label_formatter(&|d| fmt_et_label(et_depart_min + d * 86_400.0))
            .y_label_formatter(&|d| fmt_et_label(et_arrive_min + d * 86_400.0))
            .draw()?;

        for (arr_idx, row) in grid.iter().enumerate() {
            let (y0, y1) = cell_bounds(&arr_coords, arr_idx);
            for (dep_idx, &value) in row.iter().enumerate() {
                if !value.is_finite() {
                    continue;
                }
                let (x0, x1) = cell_bounds(&dep_coords, dep_idx);
                let clamped = value.clamp(min_value, high_clip);
                let t = if (high_clip - min_value).abs() < f64::EPSILON {
                    0.0
                } else {
                    (clamped - min_value) / (high_clip - min_value)
                };
                let color = jet_color(t);
                chart.draw_series(std::iter::once(Rectangle::new(
                    [(x0, y0), (x1, y1)],
                    color.filled(),
                )))?;
            }
        }

        draw_contours(&mut chart, &grid_clamped, &dep_coords, &arr_coords, &levels)?;

        let x = dep_coords[min_dep_idx];
        let y = arr_coords[min_arr_idx];
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(x, 0.0), (x, arrive_span_days)],
            ShapeStyle::from(&BLACK.mix(0.5)).stroke_width(1),
        )))?;
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(0.0, y), (depart_span_days, y)],
            ShapeStyle::from(&BLACK.mix(0.5)).stroke_width(1),
        )))?;
        let marker_color = RGBColor(210, 100, 20);
        let cross_half_width = depart_span_days * 0.02;
        let cross_half_height = arrive_span_days * 0.02;
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(x - cross_half_width, y), (x + cross_half_width, y)],
            ShapeStyle::from(&marker_color).stroke_width(3),
        )))?;
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(x, y - cross_half_height), (x, y + cross_half_height)],
            ShapeStyle::from(&marker_color).stroke_width(3),
        )))?;
        let (annotation_prefix, annotation_suffix) = metric_annotation(&metric_column);
        let text = format!("{}{:.2}{}", annotation_prefix, min_value, annotation_suffix);
        let text_pos = (x + 0.02 * depart_span_days, y + 0.02 * arrive_span_days);
        chart.draw_series(std::iter::once(Text::new(
            text,
            text_pos,
            label_font.clone().color(&marker_color),
        )))?;
    }

    {
        let mut chart = ChartBuilder::on(&legend_area)
            .margin_left(20)
            .margin_right(20)
            .margin_top(30)
            .margin_bottom(30)
            .x_label_area_size(0)
            .y_label_area_size(70)
            .build_cartesian_2d(0.0..1.0, min_value..high_clip)?;

        for i in 0..300 {
            let t0 = i as f64 / 300.0;
            let t1 = (i + 1) as f64 / 300.0;
            let v0 = min_value + (high_clip - min_value) * t0;
            let v1 = min_value + (high_clip - min_value) * t1;
            let color = jet_color(t0);
            chart.draw_series(std::iter::once(Rectangle::new(
                [(0.0, v0), (1.0, v1)],
                color.filled(),
            )))?;
        }

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .x_labels(0)
            .y_labels(6)
            .y_desc(metric_axis_label(&metric_column))
            .y_label_style(label_font.clone())
            .axis_desc_style(label_font.clone())
            .y_label_formatter(&|v| format!("{v:.2}"))
            .draw()?;
    }

    root.present()?;
    Ok(())
}

fn select_font_family() -> FontFamily<'static> {
    if cfg!(target_os = "macos") {
        FontFamily::Name("Helvetica")
    } else if cfg!(target_os = "windows") {
        FontFamily::Name("Arial")
    } else {
        FontFamily::Name("DejaVu Sans")
    }
}

fn read_cells(
    path: &str,
    metric_name: &str,
) -> anyhow::Result<(Vec<Cell>, Vec<f64>, Vec<f64>, String)> {
    let mut rdr = ReaderBuilder::new().has_headers(true).from_path(path)?;
    let headers = rdr.headers()?.clone();
    let depart_idx = headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case("depart_et"))
        .ok_or_else(|| anyhow::anyhow!("CSV missing 'depart_et' column"))?;
    let arrive_idx = headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case("arrive_et"))
        .ok_or_else(|| anyhow::anyhow!("CSV missing 'arrive_et' column"))?;
    let feasible_idx = headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case("feasible"))
        .ok_or_else(|| anyhow::anyhow!("CSV missing 'feasible' column"))?;
    let metric_idx = resolve_metric_column(&headers, metric_name)
        .ok_or_else(|| anyhow::anyhow!("CSV missing metric column matching '{}'", metric_name))?;
    let metric_column = headers
        .get(metric_idx)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Invalid metric column index"))?;

    let mut cells = Vec::new();
    let mut dep_vals = Vec::new();
    let mut arr_vals = Vec::new();
    for rec in rdr.records() {
        let r = rec?;
        let depart_et: f64 = r.get(depart_idx).unwrap_or("").parse().unwrap_or(f64::NAN);
        let arrive_et: f64 = r.get(arrive_idx).unwrap_or("").parse().unwrap_or(f64::NAN);
        let feasible = r
            .get(feasible_idx)
            .unwrap_or("false")
            .eq_ignore_ascii_case("true");
        let metric_value: f64 = r.get(metric_idx).unwrap_or("").parse().unwrap_or(f64::NAN);
        if depart_et.is_finite() && arrive_et.is_finite() {
            if feasible && metric_value.is_finite() {
                dep_vals.push(depart_et);
                arr_vals.push(arrive_et);
                cells.push(Cell {
                    depart_et,
                    arrive_et,
                    metric_value,
                });
            }
        }
    }
    Ok((cells, dep_vals, arr_vals, metric_column))
}

fn fmt_et_label(et: f64) -> String {
    match solar_travel_calculator::ephemeris::format_epoch(et) {
        Ok(epoch) => match NaiveDateTime::parse_from_str(&epoch, "%Y %b %d %H:%M:%S%.f") {
            Ok(dt) => dt.format("%Y-%m-%d").to_string(),
            Err(_) => epoch,
        },
        Err(_) => format!("{et:.0}"),
    }
}

fn jet_color(t_in: f64) -> RGBColor {
    let t = t_in.clamp(0.0, 1.0);
    fn comp(v: f64) -> f64 {
        (1.0 - (v - 1.0).abs()).clamp(0.0, 1.0)
    }
    let r = comp(1.5 - 4.0 * (t - 0.75).abs());
    let g = comp(1.5 - 4.0 * (t - 0.5).abs());
    let b = comp(1.5 - 4.0 * (t - 0.25).abs());
    RGBColor((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn build_grid(cells: &[Cell], dep_vals: &[f64], arr_vals: &[f64]) -> Vec<Vec<f64>> {
    let mut grid = vec![vec![f64::NAN; dep_vals.len()]; arr_vals.len()];
    for cell in cells {
        let dep_idx = match dep_vals.binary_search_by(|x| x.partial_cmp(&cell.depart_et).unwrap()) {
            Ok(idx) => idx,
            Err(_) => continue,
        };
        let arr_idx = match arr_vals.binary_search_by(|x| x.partial_cmp(&cell.arrive_et).unwrap()) {
            Ok(idx) => idx,
            Err(_) => continue,
        };
        let slot = &mut grid[arr_idx][dep_idx];
        if !slot.is_finite() || cell.metric_value < *slot {
            *slot = cell.metric_value;
        }
    }
    grid
}

fn draw_contours<DB: DrawingBackend>(
    chart: &mut ChartContext<DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    grid: &[Vec<f64>],
    dep_coords: &[f64],
    arr_coords: &[f64],
    levels: &[f64],
) -> Result<(), DrawingAreaErrorKind<DB::ErrorType>> {
    if dep_coords.len() < 2 || arr_coords.len() < 2 {
        return Ok(());
    }
    let min_level = *levels.first().unwrap_or(&0.0);
    let max_level = *levels.last().unwrap_or(&1.0);
    for &level in levels {
        let t = if (max_level - min_level).abs() < f64::EPSILON {
            0.0
        } else {
            (level - min_level) / (max_level - min_level)
        };
        let color = jet_color(t);
        for i in 0..arr_coords.len() - 1 {
            for j in 0..dep_coords.len() - 1 {
                let v0 = grid[i][j];
                let v1 = grid[i][j + 1];
                let v2 = grid[i + 1][j + 1];
                let v3 = grid[i + 1][j];
                if !(v0.is_finite() && v1.is_finite() && v2.is_finite() && v3.is_finite()) {
                    continue;
                }
                let coords = [
                    (dep_coords[j], arr_coords[i]),
                    (dep_coords[j + 1], arr_coords[i]),
                    (dep_coords[j + 1], arr_coords[i + 1]),
                    (dep_coords[j], arr_coords[i + 1]),
                ];
                for (p1, p2) in marching_square_segments([v0, v1, v2, v3], coords, level) {
                    chart.draw_series(std::iter::once(PathElement::new(
                        vec![p1, p2],
                        ShapeStyle::from(&color).stroke_width(1),
                    )))?;
                }
            }
        }
    }
    Ok(())
}

fn marching_square_segments(
    values: [f64; 4],
    coords: [(f64, f64); 4],
    level: f64,
) -> Vec<((f64, f64), (f64, f64))> {
    let mut idx = 0u8;
    if values[0] >= level {
        idx |= 1;
    }
    if values[1] >= level {
        idx |= 2;
    }
    if values[2] >= level {
        idx |= 4;
    }
    if values[3] >= level {
        idx |= 8;
    }
    if idx == 0 || idx == 15 {
        return Vec::new();
    }

    let edge_point = |a: usize, b: usize| -> (f64, f64) {
        let va = values[a];
        let vb = values[b];
        let (xa, ya) = coords[a];
        let (xb, yb) = coords[b];
        if (vb - va).abs() < f64::EPSILON {
            return ((xa + xb) * 0.5, (ya + yb) * 0.5);
        }
        let t = (level - va) / (vb - va);
        (xa + t * (xb - xa), ya + t * (yb - ya))
    };

    let mut segments = Vec::new();
    let mut add = |e1: usize, e2: usize| {
        let p1 = match e1 {
            0 => edge_point(0, 1),
            1 => edge_point(1, 2),
            2 => edge_point(2, 3),
            3 => edge_point(3, 0),
            _ => unreachable!(),
        };
        let p2 = match e2 {
            0 => edge_point(0, 1),
            1 => edge_point(1, 2),
            2 => edge_point(2, 3),
            3 => edge_point(3, 0),
            _ => unreachable!(),
        };
        segments.push((p1, p2));
    };

    match idx {
        1 => add(3, 0),
        2 => add(0, 1),
        3 => add(3, 1),
        4 => add(1, 2),
        5 => {
            add(3, 2);
            add(0, 1);
        }
        6 => add(0, 2),
        7 => add(3, 2),
        8 => add(2, 3),
        9 => add(2, 0),
        10 => {
            add(3, 0);
            add(1, 2);
        }
        11 => add(1, 3),
        12 => add(1, 3),
        13 => add(1, 0),
        14 => add(0, 3),
        _ => {}
    }

    segments
}

fn resolve_metric_column(headers: &csv::StringRecord, metric_name: &str) -> Option<usize> {
    let direct = headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case(metric_name));
    if direct.is_some() {
        return direct;
    }
    let metric_lower = metric_name.to_lowercase();
    let fallback = match metric_lower.as_str() {
        "dv_total" => "dv_total_km_s",
        "c3" => "c3_km2_s2",
        other => other,
    };
    headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case(fallback))
}

fn metric_annotation(metric_column: &str) -> (String, String) {
    match metric_column.to_lowercase().as_str() {
        "dv_total_km_s" => ("Δv = ".to_string(), " km/s".to_string()),
        "c3_km2_s2" => ("C3 = ".to_string(), " km^2/s^2".to_string()),
        other => (format!("{other} = "), "".to_string()),
    }
}

fn metric_axis_label(metric_column: &str) -> String {
    match metric_column.to_lowercase().as_str() {
        "dv_total_km_s" => "Total Δv (km/s)".to_string(),
        "c3_km2_s2" => "C3 (km^2/s^2)".to_string(),
        other => other.to_string(),
    }
}

fn cell_bounds(coords: &[f64], idx: usize) -> (f64, f64) {
    let center = coords[idx];
    let prev = idx.checked_sub(1).and_then(|i| coords.get(i)).copied();
    let next = coords.get(idx + 1).copied();

    let left = match (prev, next) {
        (Some(prev), _) => 0.5 * (prev + center),
        (None, Some(next)) => center - 0.5 * (next - center),
        (None, None) => center - 0.5,
    };

    let right = match (prev, next) {
        (_, Some(next)) => 0.5 * (center + next),
        (Some(prev), None) => center + 0.5 * (center - prev),
        (None, None) => center + 0.5,
    };

    (left, right)
}
