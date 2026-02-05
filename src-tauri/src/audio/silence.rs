/// Silence detection for automatic recording stop.
///
/// Uses RMS (Root Mean Square) to detect audio levels and track
/// consecutive silent frames to determine when speech has ended.

/// Default silence threshold in RMS units (0.0 to 1.0)
/// 0.01 is a reasonable default for typical microphone input
pub const DEFAULT_SILENCE_THRESHOLD: f32 = 0.01;

/// Default duration of silence before auto-stop (in seconds)
pub const DEFAULT_SILENCE_DURATION: f32 = 1.5;

/// Minimum allowed silence threshold
pub const MIN_SILENCE_THRESHOLD: f32 = 0.001;

/// Maximum allowed silence threshold
pub const MAX_SILENCE_THRESHOLD: f32 = 0.1;

/// Minimum allowed silence duration (seconds)
pub const MIN_SILENCE_DURATION: f32 = 0.5;

/// Maximum allowed silence duration (seconds)
pub const MAX_SILENCE_DURATION: f32 = 5.0;

/// Silence detector that tracks audio levels and detects extended silence.
#[derive(Debug, Clone)]
pub struct SilenceDetector {
    /// RMS threshold below which audio is considered silent
    threshold: f32,
    /// Sample rate of the audio stream
    sample_rate: u32,
    /// Number of consecutive silent samples needed to trigger
    samples_needed: usize,
    /// Current count of consecutive silent samples
    silent_samples: usize,
    /// Whether speech has been detected at least once
    speech_detected: bool,
    /// Whether auto-stop has been triggered
    triggered: bool,
}

impl SilenceDetector {
    /// Create a new silence detector.
    ///
    /// # Arguments
    /// * `threshold` - RMS level below which audio is silent (0.001 to 0.1)
    /// * `duration_secs` - Seconds of silence before auto-stop (0.5 to 5.0)
    /// * `sample_rate` - Audio sample rate (e.g., 16000)
    pub fn new(threshold: f32, duration_secs: f32, sample_rate: u32) -> Self {
        let clamped_threshold = threshold.clamp(MIN_SILENCE_THRESHOLD, MAX_SILENCE_THRESHOLD);
        let clamped_duration = duration_secs.clamp(MIN_SILENCE_DURATION, MAX_SILENCE_DURATION);

        // Calculate how many samples we need for the specified duration
        let samples_needed = (sample_rate as f32 * clamped_duration) as usize;

        Self {
            threshold: clamped_threshold,
            sample_rate,
            samples_needed,
            silent_samples: 0,
            speech_detected: false,
            triggered: false,
        }
    }

    /// Create a silence detector with default settings.
    pub fn with_defaults(sample_rate: u32) -> Self {
        Self::new(DEFAULT_SILENCE_THRESHOLD, DEFAULT_SILENCE_DURATION, sample_rate)
    }

    /// Process a chunk of audio samples and return whether auto-stop should trigger.
    ///
    /// # Arguments
    /// * `samples` - Audio samples (f32, mono)
    ///
    /// # Returns
    /// * `true` if silence duration exceeded (auto-stop should trigger)
    /// * `false` otherwise
    pub fn process(&mut self, samples: &[f32]) -> bool {
        if self.triggered || samples.is_empty() {
            return self.triggered;
        }

        let rms = calculate_rms(samples);
        let is_silent = rms < self.threshold;

        if is_silent {
            self.silent_samples += samples.len();

            // Only trigger if we've detected speech before
            // This prevents triggering on initial silence before user speaks
            if self.speech_detected && self.silent_samples >= self.samples_needed {
                self.triggered = true;
                tracing::info!(
                    "Silence detected for {:.1}s (threshold: {:.4}), triggering auto-stop",
                    self.silent_samples as f32 / self.sample_rate as f32,
                    self.threshold
                );
            }
        } else {
            // Speech detected - reset silence counter
            self.silent_samples = 0;
            self.speech_detected = true;
        }

        self.triggered
    }

    /// Check if auto-stop has been triggered.
    pub fn is_triggered(&self) -> bool {
        self.triggered
    }

    /// Check if speech has been detected at least once.
    pub fn has_speech(&self) -> bool {
        self.speech_detected
    }

    /// Get the current silence duration in seconds.
    pub fn silence_duration(&self) -> f32 {
        self.silent_samples as f32 / self.sample_rate as f32
    }

    /// Get the current RMS threshold.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Reset the detector state (but keep configuration).
    pub fn reset(&mut self) {
        self.silent_samples = 0;
        self.speech_detected = false;
        self.triggered = false;
    }
}

/// Calculate the Root Mean Square (RMS) of audio samples.
///
/// RMS provides a measure of the "average" signal level,
/// which correlates with perceived loudness better than peak values.
pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_of_squares: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_of_squares / samples.len() as f32).sqrt()
}

/// Calculate decibels from RMS value (relative to full scale).
pub fn rms_to_db(rms: f32) -> f32 {
    if rms <= 0.0 {
        return f32::NEG_INFINITY;
    }
    20.0 * rms.log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_rms_silence() {
        let samples = vec![0.0; 1000];
        assert_eq!(calculate_rms(&samples), 0.0);
    }

    #[test]
    fn test_calculate_rms_constant() {
        // Constant signal: RMS equals the absolute value
        let samples = vec![0.5; 1000];
        let rms = calculate_rms(&samples);
        assert!((rms - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_calculate_rms_sine_wave() {
        // Sine wave: RMS = peak / sqrt(2) ≈ 0.707 * peak
        let samples: Vec<f32> = (0..16000)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI / 16000.0).sin())
            .collect();
        let rms = calculate_rms(&samples);
        let expected = 1.0 / std::f32::consts::SQRT_2;
        assert!((rms - expected).abs() < 0.01);
    }

    #[test]
    fn test_calculate_rms_empty() {
        let samples: Vec<f32> = vec![];
        assert_eq!(calculate_rms(&samples), 0.0);
    }

    #[test]
    fn test_silence_detector_new() {
        let detector = SilenceDetector::new(0.02, 1.5, 16000);
        assert!((detector.threshold() - 0.02).abs() < f32::EPSILON);
        assert!(!detector.is_triggered());
        assert!(!detector.has_speech());
    }

    #[test]
    fn test_silence_detector_threshold_clamping() {
        // Too low
        let detector = SilenceDetector::new(0.0001, 1.5, 16000);
        assert!((detector.threshold() - MIN_SILENCE_THRESHOLD).abs() < f32::EPSILON);

        // Too high
        let detector = SilenceDetector::new(0.5, 1.5, 16000);
        assert!((detector.threshold() - MAX_SILENCE_THRESHOLD).abs() < f32::EPSILON);
    }

    #[test]
    fn test_silence_detector_no_trigger_without_speech() {
        // Should not trigger if no speech detected first
        let mut detector = SilenceDetector::new(0.01, 0.5, 16000);

        // Send 2 seconds of silence
        let silence = vec![0.0; 16000];
        for _ in 0..4 {
            detector.process(&silence);
        }

        // Should not trigger because no speech was detected
        assert!(!detector.is_triggered());
    }

    #[test]
    fn test_silence_detector_triggers_after_speech() {
        let mut detector = SilenceDetector::new(0.01, 0.5, 16000);

        // Send some speech (loud signal)
        let speech: Vec<f32> = (0..8000)
            .map(|i| 0.5 * (i as f32 * 0.1).sin())
            .collect();
        detector.process(&speech);
        assert!(detector.has_speech());
        assert!(!detector.is_triggered());

        // Send silence - 0.5s at 16kHz = 8000 samples needed
        // Send slightly less than threshold first
        let silence = vec![0.0; 4000]; // 0.25s - not enough yet
        assert!(!detector.process(&silence));

        // Send more silence to exceed threshold
        let more_silence = vec![0.0; 8000]; // Another 0.5s, total > 0.5s
        assert!(detector.process(&more_silence)); // Should trigger now
    }

    #[test]
    fn test_silence_detector_resets_on_speech() {
        let mut detector = SilenceDetector::new(0.01, 0.5, 16000);

        // Speech first
        let speech: Vec<f32> = vec![0.5; 4000];
        detector.process(&speech);

        // Some silence
        let silence = vec![0.0; 4000];
        detector.process(&silence);

        // More speech - should reset counter
        detector.process(&speech);
        assert_eq!(detector.silent_samples, 0);

        // Silence again - counter starts fresh
        detector.process(&silence);
        assert!(!detector.is_triggered());
    }

    #[test]
    fn test_silence_detector_reset() {
        let mut detector = SilenceDetector::new(0.01, 0.5, 16000);

        // Trigger the detector
        let speech: Vec<f32> = vec![0.5; 4000];
        detector.process(&speech);
        let silence = vec![0.0; 16000];
        detector.process(&silence);

        // Reset and verify
        detector.reset();
        assert!(!detector.is_triggered());
        assert!(!detector.has_speech());
        assert_eq!(detector.silence_duration(), 0.0);
    }

    #[test]
    fn test_silence_duration() {
        let mut detector = SilenceDetector::new(0.01, 2.0, 16000);

        // Speak first
        let speech: Vec<f32> = vec![0.5; 4000];
        detector.process(&speech);

        // 0.5 seconds of silence
        let silence = vec![0.0; 8000];
        detector.process(&silence);

        assert!((detector.silence_duration() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_rms_to_db() {
        // Full scale (1.0) = 0 dB
        assert!((rms_to_db(1.0) - 0.0).abs() < 0.01);

        // 0.1 = -20 dB
        assert!((rms_to_db(0.1) - (-20.0)).abs() < 0.01);

        // 0.01 = -40 dB
        assert!((rms_to_db(0.01) - (-40.0)).abs() < 0.01);

        // Zero = negative infinity
        assert!(rms_to_db(0.0).is_infinite());
    }

    #[test]
    fn test_with_defaults() {
        let detector = SilenceDetector::with_defaults(16000);
        assert!((detector.threshold() - DEFAULT_SILENCE_THRESHOLD).abs() < f32::EPSILON);
    }
}
