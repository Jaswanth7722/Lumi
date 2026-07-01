//! Tests: Behavior selections and interruptions counted correctly.

use lumas_character::metrics::CharacterMetrics;

#[test]
fn test_metrics_initial_state() {
    let metrics = CharacterMetrics::new();
    assert_eq!(metrics.behavior_selections.get(), 0);
    assert_eq!(metrics.behavior_interruptions.get(), 0);
    assert_eq!(metrics.emotion_changes.get(), 0);
    assert_eq!(metrics.navigation_failures.get(), 0);
}

#[test]
fn test_behavior_selection_counted() {
    let metrics = CharacterMetrics::new();
    metrics.record_behavior_selection();
    assert_eq!(metrics.behavior_selections.get(), 1);
    metrics.record_behavior_selection();
    assert_eq!(metrics.behavior_selections.get(), 2);
}

#[test]
fn test_behavior_interruption_counted() {
    let metrics = CharacterMetrics::new();
    metrics.record_behavior_interruption();
    assert_eq!(metrics.behavior_interruptions.get(), 1);
}

#[test]
fn test_emotion_change_counted() {
    let metrics = CharacterMetrics::new();
    metrics.record_emotion_change();
    assert_eq!(metrics.emotion_changes.get(), 1);
}

#[test]
fn test_navigation_failure_counted() {
    let metrics = CharacterMetrics::new();
    metrics.record_navigation_failure();
    assert_eq!(metrics.navigation_failures.get(), 1);
}

#[test]
fn test_counter_reset() {
    use lumas_character::metrics::RtSafeCounter;
    let counter = RtSafeCounter::new();
    counter.increment();
    counter.increment();
    assert_eq!(counter.get(), 2);
    counter.reset();
    assert_eq!(counter.get(), 0);
}

#[test]
fn test_histogram_record() {
    use lumas_character::metrics::HdrHistogram;
    let hist = HdrHistogram::new();
    hist.record(100);
    hist.record(200);
    assert_eq!(hist.count(), 2);
}

#[test]
fn test_gauge_set_and_get() {
    use lumas_character::metrics::AsyncGauge;
    let gauge = AsyncGauge::new();
    gauge.set(42.5);
    assert!((gauge.get() - 42.5).abs() < 0.001);
}

#[test]
fn test_metrics_clone() {
    let metrics = CharacterMetrics::new();
    metrics.record_behavior_selection();
    let cloned = CharacterMetrics::default();
    // Default creates new, not clone
    assert_eq!(cloned.behavior_selections.get(), 0);

    // Metrics created via new() has Arc inside, so clone is cheap
    let metrics2 = metrics.clone();
    assert_eq!(metrics2.behavior_selections.get(), 1);
}
