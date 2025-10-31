use assert_cmd::Command;
use std::fs::{self, File};
use std::io::Write;

#[test]
fn porkchop_plot_renders_png() {
    let dir = tempfile::tempdir().expect("tempdir");
    let csv_path = dir.path().join("pork.csv");
    let png_path = dir.path().join("pork.png");

    let mut file = File::create(&csv_path).expect("csv create");
    writeln!(
        file,
        "depart_et,arrive_et,depart_utc,arrive_utc,tof_days,c3_km2_s2,vinf_dep_km_s,vinf_arr_km_s,dv_dep_km_s,dv_arr_km_s,dv_total_km_s,lambert_path,feasible,origin_body,dest_body,rpark_dep_km,rpark_arr_km"
    )
    .unwrap();
    for i in 0..3 {
        let depart_et = 1.0e8 + i as f64 * 10_000.0;
        let arrive_et = depart_et + 200_000.0;
        writeln!(
            file,
            "{depart_et},{arrive_et},DUTC,AUTC,{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},short,true,EARTH,MARS,6778.0,3778.0",
            150.0 + i as f64,
            12.0 + i as f64,
            3.0 + i as f64 * 0.1,
            2.4 + i as f64 * 0.1,
            1.1 + i as f64 * 0.05,
            0.9 + i as f64 * 0.05,
            2.0 + i as f64 * 0.1,
        )
        .unwrap();
    }

    Command::cargo_bin("porkchop_plot")
        .expect("porkchop_plot bin")
        .args([
            "--input",
            csv_path.to_str().unwrap(),
            "--output",
            png_path.to_str().unwrap(),
            "--metric",
            "dv_total",
            "--width",
            "400",
            "--height",
            "300",
        ])
        .assert()
        .success();

    let metadata = fs::metadata(png_path).expect("png metadata");
    assert!(metadata.len() > 0, "PNG output should not be empty");
}
