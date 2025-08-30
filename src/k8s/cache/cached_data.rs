use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CachedData<T> {
    pub data: T,
    pub last_updated: Instant,
    pub ttl: Duration,
    pub fetch_status: FetchStatus,
    pub version: u64,  // For tracking updates
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchStatus {
    Fresh,
    Stale,
    Fetching,
    Error(String),
}

impl<T> CachedData<T> {
    pub fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            last_updated: Instant::now(),
            ttl,
            fetch_status: FetchStatus::Fresh,
            version: 0,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.last_updated.elapsed() > self.ttl
    }

    pub fn is_fresh(&self) -> bool {
        !self.is_expired() && matches!(self.fetch_status, FetchStatus::Fresh)
    }

    pub fn age(&self) -> Duration {
        self.last_updated.elapsed()
    }

    pub fn update(&mut self, data: T) {
        self.data = data;
        self.last_updated = Instant::now();
        self.fetch_status = FetchStatus::Fresh;
        self.version += 1;
    }

    pub fn mark_stale(&mut self) {
        self.fetch_status = FetchStatus::Stale;
    }

    pub fn mark_fetching(&mut self) {
        self.fetch_status = FetchStatus::Fetching;
    }

    pub fn mark_error(&mut self, error: String) {
        self.fetch_status = FetchStatus::Error(error);
    }

    pub fn time_until_expiry(&self) -> Option<Duration> {
        let elapsed = self.last_updated.elapsed();
        if elapsed < self.ttl {
            Some(self.ttl - elapsed)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_cached_data_expiry() {
        let data = CachedData::new("test".to_string(), Duration::from_millis(100));
        assert!(!data.is_expired());
        assert!(data.is_fresh());
        
        sleep(Duration::from_millis(150));
        assert!(data.is_expired());
        assert!(!data.is_fresh());
    }

    #[test]
    fn test_cached_data_update() {
        let mut data = CachedData::new(1, Duration::from_secs(60));
        assert_eq!(data.version, 0);
        
        data.update(2);
        assert_eq!(data.data, 2);
        assert_eq!(data.version, 1);
        assert!(data.is_fresh());
    }

    #[test]
    fn test_fetch_status_transitions() {
        let mut data = CachedData::new(vec![1, 2, 3], Duration::from_secs(60));
        assert_eq!(data.fetch_status, FetchStatus::Fresh);
        
        data.mark_stale();
        assert_eq!(data.fetch_status, FetchStatus::Stale);
        
        data.mark_fetching();
        assert_eq!(data.fetch_status, FetchStatus::Fetching);
        
        data.mark_error("API error".to_string());
        assert!(matches!(data.fetch_status, FetchStatus::Error(_)));
    }
}