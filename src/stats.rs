use serde::{Deserialize, Serialize};
use statistical::{mean, median, standard_deviation};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub std_dev: f64,
    pub p90: f64,
    pub p99: f64,
    pub latencies: Vec<f64>,
}

impl LatencyStats {
    pub fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            median: 0.0,
            min: 0.0,
            max: 0.0,
            std_dev: 0.0,
            p90: 0.0,
            p99: 0.0,
            latencies: Vec::new(),
        }
    }

    pub fn add_latency(&mut self, latency: f64) {
        self.latencies.push(latency);
        self.count = self.latencies.len();
        
        if self.count == 1 {
            self.min = latency;
            self.max = latency;
        } else {
            if latency < self.min {
                self.min = latency;
            }
            if latency > self.max {
                self.max = latency;
            }
        }
    }

    pub fn calculate(&mut self) {
        if self.latencies.is_empty() {
            return;
        }

        self.latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        self.mean = mean(&self.latencies);
        self.median = median(&self.latencies);
        self.std_dev = standard_deviation(&self.latencies, Some(self.mean));
        self.p90 = percentile(&self.latencies, 0.90);
        self.p99 = percentile(&self.latencies, 0.99);
    }

    pub fn print_summary(&self, name: &str) {
        println!("===== {} Performance Analysis =====", name);
        println!("Sample count: {}", self.count);
        
        if self.count > 0 {
            println!("Latency statistics:");
            println!("  Average latency: {:.2}ms", self.mean);
            println!("  Minimum latency: {:.2}ms", self.min);
            println!("  Maximum latency: {:.2}ms", self.max);
            println!("  Standard deviation: {:.2}ms", self.std_dev);
            println!("  Median (p50): {:.2}ms", self.median);
            println!("  90th percentile (p90): {:.2}ms", self.p90);
            println!("  99th percentile (p99): {:.2}ms", self.p99);
        } else {
            println!("No data collected for {}", name);
        }
    }
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self::new()
    }
}

pub fn calculate_stats(latencies: &[f64]) -> LatencyStats {
    let mut stats = LatencyStats::new();
    
    for &latency in latencies {
        stats.add_latency(latency);
    }
    
    stats.calculate();
    stats
}

pub fn percentile(data: &[f64], p: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    
    let index = (data.len() as f64 * p) as usize;
    let index = if index >= data.len() {
        data.len() - 1
    } else {
        index
    };
    
    data[index]
}

#[derive(Debug, Clone)]
pub struct EndpointStats {
    pub total_latency: f64,
    pub latencies: Vec<f64>,
    pub first_received: usize,
    pub total_received: usize,
    pub is_available: bool,
    pub has_received_data: bool,
    pub first_slot: Option<u64>,
}

impl EndpointStats {
    pub fn new() -> Self {
        Self {
            total_latency: 0.0,
            latencies: Vec::new(),
            first_received: 0,
            total_received: 0,
            is_available: true,
            has_received_data: false,
            first_slot: None,
        }
    }

    pub fn add_latency(&mut self, latency: f64) {
        self.latencies.push(latency);
        self.total_latency += latency;
    }

    pub fn increment_first_received(&mut self) {
        self.first_received += 1;
    }

    pub fn increment_total_received(&mut self) {
        self.total_received += 1;
    }

    pub fn get_average_latency(&self) -> f64 {
        if self.latencies.is_empty() {
            0.0
        } else {
            self.total_latency / self.latencies.len() as f64
        }
    }

    pub fn get_first_received_percentage(&self) -> f64 {
        if self.total_received == 0 {
            0.0
        } else {
            (self.first_received as f64 / self.total_received as f64) * 100.0
        }
    }

    pub fn get_stats(&self) -> LatencyStats {
        calculate_stats(&self.latencies)
    }
}

impl Default for EndpointStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct BlockData {
    pub endpoint: String,
    pub slot: u64,
    pub timestamp: f64,
}

pub type EndpointStatsMap = HashMap<String, EndpointStats>;
pub type BlockDataBySlot = HashMap<u64, Vec<BlockData>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_stats() {
        let latencies = vec![10.0, 20.0, 15.0, 25.0, 30.0];
        let stats = calculate_stats(&latencies);
        
        assert_eq!(stats.count, 5);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 30.0);
        assert_eq!(stats.mean, 20.0);
        assert_eq!(stats.median, 20.0);
    }

    #[test]
    fn test_percentile() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&data, 0.90), 9.0);
        assert_eq!(percentile(&data, 0.50), 5.0);
    }
}