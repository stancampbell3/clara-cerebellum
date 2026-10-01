use clara_metrics::{counters, exporter, gauges, histograms};

#[test]
fn counters_render_in_prometheus_text_format() {
    let handle = exporter::init();
    counters::ritual_created("test.domain");
    let rendered = handle.render();
    assert!(rendered.contains("clara_ritual_created_total"));
    assert!(rendered.contains("test.domain"));
}

#[test]
fn gauges_render_in_prometheus_text_format() {
    let handle = exporter::init();
    gauges::ritual_active("test.domain", 3);
    let rendered = handle.render();
    assert!(rendered.contains("clara_ritual_active"));
}

#[test]
fn histograms_render_in_prometheus_text_format() {
    let handle = exporter::init();
    histograms::tool_duration("echo", std::time::Duration::from_millis(5));
    let rendered = handle.render();
    assert!(rendered.contains("clara_tool_duration_seconds"));
}
