use chrono::{DateTime, Utc, Duration};

/// Certificate validation utilities with proper date parsing
pub struct CertificateValidator {
    expiry_warning_days: i64,
}

impl Default for CertificateValidator {
    fn default() -> Self {
        Self {
            expiry_warning_days: 30,
        }
    }
}

impl CertificateValidator {
    /// Creates a new validator with custom warning period
    pub fn with_warning_days(days: i64) -> Self {
        Self {
            expiry_warning_days: days,
        }
    }
    
    /// Check if certificate expires within the warning period (or is already expired)
    pub fn is_expiring_soon(&self, expires: &str) -> bool {
        self.parse_expiry_date(expires)
            .map(|expiry| {
                let now = Utc::now();
                let warning_threshold = now + Duration::days(self.expiry_warning_days);
                expiry <= warning_threshold
            })
            .unwrap_or_else(|| self.fallback_expiry_check(expires))
    }
    
    /// Get the number of days until expiration (negative if expired)
    pub fn days_until_expiry(&self, expires: &str) -> Option<i64> {
        self.parse_expiry_date(expires)
            .map(|expiry| {
                let now = Utc::now();
                (expiry - now).num_days()
            })
    }
    
    /// Format expiration date with appropriate warnings
    pub fn format_expiration_with_warning(&self, expires: &str) -> String {
        if expires.is_empty() || expires == "-" {
            return "Unknown".to_string();
        }
        
        if let Some(days) = self.days_until_expiry(expires) {
            if days < 0 {
                format!("{} (Expired {} days ago) 🔴", expires, -days)
            } else if days <= self.expiry_warning_days {
                format!("{} ({} days left) ⚠️", expires, days)
            } else {
                expires.to_string()
            }
        } else if self.fallback_expiry_check(expires) {
            format!("{} ⚠️", expires)
        } else {
            expires.to_string()
        }
    }
    
    /// Parse various date formats commonly found in certificate data
    fn parse_expiry_date(&self, expires: &str) -> Option<DateTime<Utc>> {
        // Try multiple common formats
        let formats = [
            "%Y-%m-%d %H:%M:%S %Z",     // 2024-12-31 23:59:59 UTC
            "%Y-%m-%d %H:%M:%S",        // 2024-12-31 23:59:59
            "%Y-%m-%dT%H:%M:%SZ",       // ISO 8601 UTC
            "%Y-%m-%dT%H:%M:%S%z",      // ISO 8601 with timezone
            "%Y-%m-%d",                 // 2024-12-31
            "%b %d %Y",                 // Dec 31 2024
            "%b %d, %Y",                // Dec 31, 2024
            "%d %b %Y",                 // 31 Dec 2024
            "%d/%m/%Y",                 // 31/12/2024
            "%m/%d/%Y",                 // 12/31/2024
        ];
        
        // Clean up the input string
        let cleaned = expires.trim().replace("  ", " ");
        
        // Try parsing with each format
        for format in &formats {
            if let Ok(dt) = DateTime::parse_from_str(&cleaned, format) {
                return Some(dt.with_timezone(&Utc));
            }
        }
        
        // Try chrono's flexible parser
        if let Ok(dt) = cleaned.parse::<DateTime<Utc>>() {
            return Some(dt);
        }
        
        None
    }
    
    /// Fallback string-based heuristic for when date parsing fails
    fn fallback_expiry_check(&self, expires: &str) -> bool {
        let expires_lower = expires.to_lowercase();
        
        // Check for explicit expiration indicators
        if expires_lower.contains("expired") || 
           expires_lower.contains("invalid") ||
           expires_lower.contains("revoked") {
            return true;
        }
        
        // Look for patterns like "X days" with small numbers
        if expires_lower.contains("days") {
            // Extract numbers and check if any are small
            let numbers: Vec<i32> = expires
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            
            for num in numbers {
                if num >= 0 && num <= self.expiry_warning_days as i32 {
                    return true;
                }
            }
        }
        
        // Look for "soon" indicators
        expires_lower.contains("soon") || 
        expires_lower.contains("warning") ||
        expires_lower.contains("expiring")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    
    #[test]
    fn test_iso_date_parsing() {
        let validator = CertificateValidator::default();
        
        // Test valid ISO dates
        assert!(validator.parse_expiry_date("2024-12-31T23:59:59Z").is_some());
        assert!(validator.parse_expiry_date("2024-01-01T00:00:00Z").is_some());
    }
    
    #[test]
    fn test_expiry_detection() {
        let validator = CertificateValidator::with_warning_days(30);
        
        // Test obvious expiration
        assert!(validator.is_expiring_soon("expired"));
        assert!(validator.is_expiring_soon("INVALID"));
        assert!(validator.is_expiring_soon("5 days"));
        
        // Test non-expiring
        assert!(!validator.is_expiring_soon("365 days"));
        assert!(!validator.is_expiring_soon("valid"));
    }
    
    #[test]
    fn test_days_until_expiry() {
        let validator = CertificateValidator::default();
        
        // Create a future date
        let future = Utc::now() + Duration::days(10);
        let future_str = future.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        
        let days = validator.days_until_expiry(&future_str);
        assert!(days.is_some());
        assert!(days.unwrap() >= 9 && days.unwrap() <= 10); // Account for timing
    }
    
    #[test]
    fn test_format_with_warning() {
        let validator = CertificateValidator::with_warning_days(30);
        
        // Test formatting
        let result = validator.format_expiration_with_warning("expired");
        assert!(result.contains("⚠️"));
        
        let result = validator.format_expiration_with_warning("Unknown");
        assert_eq!(result, "Unknown");
        
        let result = validator.format_expiration_with_warning("-");
        assert_eq!(result, "Unknown");
    }
}