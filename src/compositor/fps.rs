// FPS counter for performance measurement
use std::time::{Duration, Instant};

/// Simple rolling-average FPS counter
pub struct FpsCounter {
    /// When we started counting
    last_report: Instant,
    /// Frames since last report
    frame_count: u32,
    /// Last calculated FPS
    current_fps: f64,
    /// Report interval
    report_interval: Duration,
}

impl FpsCounter {
    pub fn new() -> Self {
        Self {
            last_report: Instant::now(),
            frame_count: 0,
            current_fps: 0.0,
            report_interval: Duration::from_millis(500),
        }
    }

    /// Call this after each frame render
    /// Returns Some(fps) if a new measurement is available
    pub fn tick(&mut self) -> Option<f64> {
        self.frame_count += 1;
        
        let elapsed = self.last_report.elapsed();
        if elapsed >= self.report_interval {
            self.current_fps = self.frame_count as f64 / elapsed.as_secs_f64();
            self.frame_count = 0;
            self.last_report = Instant::now();
            Some(self.current_fps)
        } else {
            None
        }
    }

    /// Get the last calculated FPS
    pub fn fps(&self) -> f64 {
        self.current_fps
    }
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self::new()
    }
}
