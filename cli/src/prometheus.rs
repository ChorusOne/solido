//! Utilities for formatting Prometheus metrics.
//!
//! See also https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format.

use std::io::Write;
use std::io;

pub struct MetricFamily<'a> {
    /// Name of the metric, e.g. `goats_teleported_total`.
    pub name: &'a str,
    /// HELP line content.
    pub help: &'a str,
    /// TYPE line content. Most common are `counter`, `gauge`, and `histogram`.
    pub type_: &'a str,
    /// Values for this metric, possibly with labels or a suffix.
    pub metrics: Vec<Metric<'a>>
}

pub struct Metric<'a> {
    /// Suffix to append to the metric name, useful for e.g. the `_bucket` suffix on histograms.
    pub suffix: &'a str,
    /// Name-value label pairs.
    pub labels: Vec<(&'a str, &'a str)>,
    /// Metric value.
    pub value: u64,
}

impl<'a> Metric<'a> {
    /// Just a value, no labels.
    pub fn simple(value: u64) -> Metric<'a> {
        Metric { labels: Vec::new(), suffix: "", value }
    }

    /// A suffix and value but no labels.
    pub fn suffix(suffix: &'a str, value: u64) -> Metric<'a> {
        Metric { labels: Vec::new(), suffix, value }
    }

    /// A value, and a single key-value label.
    pub fn singleton(label_key: &'a str, label_value: &'a str, value: u64) -> Metric<'a> {
        Metric { labels: vec![(label_key, label_value)], suffix: "", value }
    }

    /// A value, and a single key-value label.
    pub fn suffix_singleton(suffix: &'a str, label_key: &'a str, label_value: &'a str, value: u64) -> Metric<'a> {
        Metric { labels: vec![(label_key, label_value)], suffix, value }
    }
}

pub fn write_metric<W: Write>(out: &mut W, family: &MetricFamily) -> io::Result<()> {
    writeln!(out, "# HELP {} {}", family.name, family.help)?;
    writeln!(out, "# TYPE {} {}", family.name, family.type_)?;
    for metric in family.metrics.iter() {
        write!(out, "{}{}", family.name, metric.suffix)?;

        // If there are labels, write the key-value pairs between {}.
        // Escaping of the value uses Rust's string syntax, which is
        // not exactly what Prometheus wants, but it is identical for
        // all of the values that we use it with; this is not a general
        // Prometheus formatter, just a quick one for our use.
        if metric.labels.len() > 0 {
            write!(out, "{{")?;
            let mut separator = "";
            for (key, value)  in metric.labels.iter() {
                write!(out, "{}{}={:?}", separator, key, value)?;
                separator = ",";
            }
            write!(out, "}}")?;
        }

        writeln!(out, " {}", metric.value)?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::{MetricFamily, Metric, write_metric};
    use std::str;

    #[test]
    fn write_metric_without_labels() {
        let mut out: Vec<u8> = Vec::new();
        write_metric(&mut out, &MetricFamily {
            name: "goats_teleported_total",
            help: "Number of goats teleported since launch.",
            type_: "counter",
            metrics: vec![Metric::simple(144)],
        }).unwrap();

        assert_eq!(
            str::from_utf8(&out[..]),
            Ok(
                "# HELP goats_teleported_total Number of goats teleported since launch.\n\
                 # TYPE goats_teleported_total counter\n\
                 goats_teleported_total 144\n\
                "
            )
        )
    }

    #[test]
    fn write_metric_histogram() {
        let mut out: Vec<u8> = Vec::new();
        write_metric(&mut out, &MetricFamily {
            name: "teleported_goat_weight_kg",
            help: "Histogram of the weight of teleported goats.",
            type_: "histogram",
            metrics: vec![
                Metric::suffix_singleton("_bucket", "le", "50.0", 44),
                Metric::suffix_singleton("_bucket", "le", "75.0", 67),
                Metric::suffix_singleton("_bucket", "le", "+Inf", 144),
                Metric::suffix("_sum", 11520),
                Metric::suffix("_count", 144),
            ],
        }).unwrap();

        assert_eq!(
            str::from_utf8(&out[..]),
            Ok(
                "# HELP teleported_goat_weight_kg Histogram of the weight of teleported goats.\n\
                 # TYPE teleported_goat_weight_kg histogram\n\
                 teleported_goat_weight_kg_bucket{le=\"50.0\"} 44\n\
                 teleported_goat_weight_kg_bucket{le=\"75.0\"} 67\n\
                 teleported_goat_weight_kg_bucket{le=\"+Inf\"} 144\n\
                 teleported_goat_weight_kg_sum 11520\n\
                 teleported_goat_weight_kg_count 144\n\
                "
            )
        )
    }

    #[test]
    fn write_metric_multiple_labels() {
        let mut out: Vec<u8> = Vec::new();
        write_metric(&mut out, &MetricFamily {
            name: "goats_teleported_total",
            help: "Number of goats teleported since launch by departure and arrival.",
            type_: "counter",
            metrics: vec![
                Metric { suffix: "", labels: vec![("src", "AMS"), ("dst", "ZRH")], value: 10 },
                Metric { suffix: "", labels: vec![("src", "ZRH"), ("dst", "DXB")], value: 53 },
            ],
        }).unwrap();

        assert_eq!(
            str::from_utf8(&out[..]),
            Ok(
                "# HELP goats_teleported_total Number of goats teleported since launch by departure and arrival.\n\
                 # TYPE goats_teleported_total counter\n\
                 goats_teleported_total{src=\"AMS\",dst=\"ZRH\"} 10\n\
                 goats_teleported_total{src=\"ZRH\",dst=\"DXB\"} 53\n\
                "
            )
        )
    }
}