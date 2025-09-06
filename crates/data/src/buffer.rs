use std::collections::VecDeque;
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq)]
pub struct PriceDataPoint {
    pub ts: DateTime<Utc>,
    pub price: f64,
    pub volume: Option<f64>,
}

pub struct PriceDataBuffer {
    cap: usize,
    buf: VecDeque<PriceDataPoint>,
}

impl PriceDataBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "PriceDataBuffer capacity must be > 0");
        Self { cap: capacity, buf: VecDeque::with_capacity(capacity) }
    }

    pub fn capacity(&self) -> usize { self.cap }
    pub fn len(&self) -> usize { self.buf.len() }
    pub fn is_empty(&self) -> bool { self.buf.is_empty() }

    pub fn push(&mut self, point: PriceDataPoint) {
        self.buf.push_back(point);
        while self.buf.len() > self.cap {
            self.buf.pop_front();
        }
    }

    pub fn push_raw(&mut self, ts: DateTime<Utc>, price: f64, volume: Option<f64>) {
        self.push(PriceDataPoint { ts, price, volume });
    }

    pub fn last(&self) -> Option<&PriceDataPoint> { self.buf.back() }

    pub fn prune_older_than(&mut self, cutoff: DateTime<Utc>) {
        while let Some(front) = self.buf.front() {
            if front.ts < cutoff { self.buf.pop_front(); } else { break; }
        }
    }

    pub fn as_vec(&self) -> Vec<PriceDataPoint> {
        self.buf.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn ring_behavior_capacity() {
        let start = Utc::now();
        let mut b = PriceDataBuffer::new(3);
        for i in 0..5 {
            b.push_raw(start + Duration::seconds(i), 100.0 + i as f64, None);
        }
        assert_eq!(b.len(), 3);
        assert_eq!(b.as_vec().first().unwrap().price, 102.0);
        assert_eq!(b.last().unwrap().price, 104.0);
    }

    #[test]
    fn prune_by_time_keeps_recent() {
        let start = Utc::now();
        let mut b = PriceDataBuffer::new(10);
        for i in 0..5 {
            b.push_raw(start + Duration::seconds(i), i as f64, None);
        }
        b.prune_older_than(start + Duration::seconds(3));
        assert_eq!(b.len(), 2);
        assert_eq!(b.as_vec().first().unwrap().price, 3.0);
    }
}
