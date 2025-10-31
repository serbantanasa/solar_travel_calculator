//! Export helpers for CSV and JSON artifacts.

pub mod porkchop {
    use std::fs::{self, File};
    use std::io::{self, BufWriter, Write};
    use std::path::Path;

    const HEADER: &str = "depart_et,arrive_et,depart_utc,arrive_utc,tof_days,c3_km2_s2,vinf_dep_km_s,vinf_arr_km_s,dv_dep_km_s,dv_arr_km_s,dv_total_km_s,propellant_used_kg,burn_time_s,final_mass_kg,lambert_path,feasible,origin_body,dest_body,rpark_dep_km,rpark_arr_km";

    /// Create a writer for the target path, handling stdout (`-`) by convention.
    pub fn writer_for_path(path: &Path) -> io::Result<Box<dyn Write>> {
        if path == Path::new("-") {
            return Ok(Box::new(BufWriter::new(io::stdout())));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = File::create(path)?;
        Ok(Box::new(BufWriter::new(file)))
    }

    /// Write the standard porkchop CSV header.
    pub fn write_header(writer: &mut dyn Write) -> io::Result<()> {
        writeln!(writer, "{}", HEADER)
    }

    /// CSV row emitted by the porkchop exporter.
    #[derive(Debug, Clone)]
    pub struct Record<'a> {
        pub depart_et: f64,
        pub arrive_et: f64,
        pub depart_utc: &'a str,
        pub arrive_utc: &'a str,
        pub tof_days: f64,
        pub c3: f64,
        pub vinf_dep: f64,
        pub vinf_arr: f64,
        pub dv_dep: f64,
        pub dv_arr: f64,
        pub dv_total: f64,
        pub propellant_used_kg: f64,
        pub burn_time_s: f64,
        pub final_mass_kg: f64,
        pub path: &'a str,
        pub feasible: bool,
        pub origin_body: &'a str,
        pub dest_body: &'a str,
        pub rpark_dep_km: f64,
        pub rpark_arr_km: f64,
    }

    impl<'a> Record<'a> {
        /// Serialize the record to CSV, matching the standard header ordering.
        pub fn write_to(&self, writer: &mut dyn Write) -> io::Result<()> {
            writeln!(
                writer,
                "{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.3},{:.3},{:.3},{},{},{},{},{:.3},{:.3}",
                self.depart_et,
                self.arrive_et,
                self.depart_utc,
                self.arrive_utc,
                self.tof_days,
                self.c3,
                self.vinf_dep,
                self.vinf_arr,
                self.dv_dep,
                self.dv_arr,
                self.dv_total,
                self.propellant_used_kg,
                self.burn_time_s,
                self.final_mass_kg,
                self.path,
                if self.feasible { "true" } else { "false" },
                self.origin_body,
                self.dest_body,
                self.rpark_dep_km,
                self.rpark_arr_km,
            )
        }
    }
}

pub mod continuous {
    use serde::Serialize;
    use serde_json::to_writer_pretty;
    use std::fs::{self, File};
    use std::io;
    use std::path::Path;

    /// Telemetry sample used in exported JSON sidecars.
    #[derive(Debug, Clone, Serialize)]
    pub struct Sample {
        pub time_s: f64,
        pub distance_m: f64,
        pub velocity_m_s: f64,
        pub mass_kg: f64,
    }

    /// Envelope of continuous-thrust telemetry summary.
    #[derive(Debug, Serialize)]
    pub struct TelemetrySummary {
        pub time_of_flight_s: f64,
        pub burn_time_total_s: f64,
        pub propellant_used_kg: f64,
        pub final_mass_kg: f64,
        pub max_velocity_m_s: f64,
        pub max_velocity_fraction_c: f64,
        pub total_distance_m: f64,
        pub kinetic_energy_joules: f64,
        pub samples: Vec<Sample>,
    }

    /// Metadata describing the continuous-thrust run.
    #[derive(Debug)]
    pub struct Metadata<'a> {
        pub vehicle: &'a str,
        pub origin: &'a str,
        pub destination: &'a str,
        pub depart_et: f64,
        pub depart_utc: &'a str,
        pub arrive_et: f64,
        pub arrive_utc: &'a str,
    }

    #[derive(Serialize)]
    struct TelemetrySidecar<'a> {
        vehicle: &'a str,
        origin: &'a str,
        destination: &'a str,
        depart_et: f64,
        depart_utc: &'a str,
        arrive_et: f64,
        arrive_utc: &'a str,
        time_of_flight_s: f64,
        burn_time_total_s: f64,
        propellant_used_kg: f64,
        final_mass_kg: f64,
        max_velocity_m_s: f64,
        max_velocity_fraction_c: f64,
        total_distance_m: f64,
        kinetic_energy_joules: f64,
        samples: &'a [Sample],
    }

    #[derive(Serialize)]
    struct DailySidecar<'a> {
        vehicle: &'a str,
        origin: &'a str,
        destination: &'a str,
        depart_et: f64,
        depart_utc: &'a str,
        arrive_et: f64,
        arrive_utc: &'a str,
        samples: Vec<DailyAggregate>,
    }

    #[derive(Serialize)]
    struct DailyAggregate {
        day_index: usize,
        time_s: f64,
        distance_m: f64,
        velocity_m_s: f64,
        mass_kg: f64,
    }

    /// Write hourly and daily JSON telemetry sidecars for a continuous-thrust run.
    pub fn write_sidecars(
        output: &Path,
        meta: &Metadata<'_>,
        summary: &TelemetrySummary,
    ) -> io::Result<()> {
        let parent = output.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let stem = output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("telemetry");

        let hourly_path = parent.join(format!("{}_hourly.json", stem));
        let daily_path = parent.join(format!("{}_daily.json", stem));

        let hourly = TelemetrySidecar {
            vehicle: meta.vehicle,
            origin: meta.origin,
            destination: meta.destination,
            depart_et: meta.depart_et,
            depart_utc: meta.depart_utc,
            arrive_et: meta.arrive_et,
            arrive_utc: meta.arrive_utc,
            time_of_flight_s: summary.time_of_flight_s,
            burn_time_total_s: summary.burn_time_total_s,
            propellant_used_kg: summary.propellant_used_kg,
            final_mass_kg: summary.final_mass_kg,
            max_velocity_m_s: summary.max_velocity_m_s,
            max_velocity_fraction_c: summary.max_velocity_fraction_c,
            total_distance_m: summary.total_distance_m,
            kinetic_energy_joules: summary.kinetic_energy_joules,
            samples: &summary.samples,
        };

        to_writer_pretty(File::create(&hourly_path)?, &hourly)?;

        if summary.time_of_flight_s >= 86_400.0 {
            let daily_samples = aggregate_daily(&summary.samples);
            let daily = DailySidecar {
                vehicle: meta.vehicle,
                origin: meta.origin,
                destination: meta.destination,
                depart_et: meta.depart_et,
                depart_utc: meta.depart_utc,
                arrive_et: meta.arrive_et,
                arrive_utc: meta.arrive_utc,
                samples: daily_samples,
            };
            to_writer_pretty(File::create(&daily_path)?, &daily)?;
        }

        Ok(())
    }

    fn aggregate_daily(samples: &[Sample]) -> Vec<DailyAggregate> {
        let mut daily: Vec<DailyAggregate> = Vec::new();
        let seconds_per_day = 86_400.0;
        for sample in samples {
            let day_index = (sample.time_s / seconds_per_day).floor() as usize;
            match daily.last_mut() {
                Some(last) if last.day_index == day_index => {
                    last.time_s = sample.time_s;
                    last.distance_m = sample.distance_m;
                    last.velocity_m_s = sample.velocity_m_s;
                    last.mass_kg = sample.mass_kg;
                }
                _ => daily.push(DailyAggregate {
                    day_index,
                    time_s: sample.time_s,
                    distance_m: sample.distance_m,
                    velocity_m_s: sample.velocity_m_s,
                    mass_kg: sample.mass_kg,
                }),
            }
        }
        if daily.is_empty() {
            daily.push(DailyAggregate {
                day_index: 0,
                time_s: samples.last().map(|s| s.time_s).unwrap_or_default(),
                distance_m: samples.last().map(|s| s.distance_m).unwrap_or_default(),
                velocity_m_s: samples.last().map(|s| s.velocity_m_s).unwrap_or_default(),
                mass_kg: samples.last().map(|s| s.mass_kg).unwrap_or_default(),
            });
        }
        daily
    }
}
