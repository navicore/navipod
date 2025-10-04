//! Sparkline rendering utilities for inline trend visualization

/// Sparkline characters from lowest to highest
const SPARKLINE_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render a sparkline from a series of values (0-100 scale)
///
/// # Arguments
/// * `values` - Values to render, scaled 0-100
/// * `max_width` - Maximum number of characters to render (will downsample if needed)
///
/// # Returns
/// A string containing sparkline characters
#[must_use]
pub fn render_sparkline(values: &[u8], max_width: Option<usize>) -> String {
    if values.is_empty() {
        return String::new();
    }

    // Downsample if needed
    let display_values = max_width.map_or_else(|| values.to_vec(), |width| {
        if values.len() > width {
            downsample(values, width)
        } else {
            values.to_vec()
        }
    });

    display_values
        .iter()
        .map(|&v| value_to_char(v))
        .collect()
}

/// Convert a 0-100 value to a sparkline character
fn value_to_char(value: u8) -> char {
    let clamped = value.min(100);
    let index = (clamped as usize * (SPARKLINE_CHARS.len() - 1)) / 100;
    SPARKLINE_CHARS[index]
}

/// Downsample values to fit within `max_width` by averaging
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
fn downsample(values: &[u8], target_len: usize) -> Vec<u8> {
    if values.len() <= target_len {
        return values.to_vec();
    }

    let bucket_size = values.len() as f64 / target_len as f64;
    (0..target_len)
        .map(|i| {
            let start = (i as f64 * bucket_size) as usize;
            let end = ((i + 1) as f64 * bucket_size) as usize;
            let bucket = &values[start..end.min(values.len())];
            if bucket.is_empty() {
                0
            } else {
                let sum: u32 = bucket.iter().map(|&v| u32::from(v)).sum();
                (sum / bucket.len() as u32) as u8
            }
        })
        .collect()
}

/// Render a sparkline with a trend arrow
///
/// # Arguments
/// * `values` - Values to render
/// * `trend_arrow` - Trend indicator (e.g., "↗️", "→", "↘️")
/// * `max_width` - Maximum width for sparkline
#[must_use]
pub fn render_sparkline_with_trend(
    values: &[u8],
    trend_arrow: &str,
    max_width: Option<usize>,
) -> String {
    let sparkline = render_sparkline(values, max_width);
    if sparkline.is_empty() {
        String::new()
    } else {
        format!("{sparkline} {trend_arrow}")
    }
}

/// Render a compact resource line with sparkline
///
/// Example output: "CPU: 250m/1000m [25%] ▁▂▃▄▅▆▇ ↗️"
#[must_use]
pub fn render_resource_with_sparkline(
    usage: &str,
    limit: &str,
    percentage: f64,
    sparkline_values: &[u8],
    trend_arrow: &str,
) -> String {
    let sparkline = render_sparkline(sparkline_values, Some(10));
    if sparkline.is_empty() {
        format!("{usage}/{limit} [{percentage:.0}%]")
    } else {
        format!("{usage}/{limit} [{percentage:.0}%] {sparkline} {trend_arrow}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_char() {
        assert_eq!(value_to_char(0), '▁');
        assert_eq!(value_to_char(50), '▄');
        assert_eq!(value_to_char(100), '█');
    }

    #[test]
    fn test_render_sparkline_empty() {
        assert_eq!(render_sparkline(&[], None), "");
    }

    #[test]
    fn test_render_sparkline_simple() {
        let values = vec![0, 25, 50, 75, 100];
        let result = render_sparkline(&values, None);
        assert_eq!(result.chars().count(), 5);
        assert!(result.starts_with('▁'));
        assert!(result.ends_with('█'));
    }

    #[test]
    fn test_render_sparkline_with_max_width() {
        let values = vec![0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        let result = render_sparkline(&values, Some(5));
        assert_eq!(result.chars().count(), 5);
    }

    #[test]
    fn test_downsample() {
        let values = vec![10, 20, 30, 40, 50, 60];
        let result = downsample(&values, 3);
        assert_eq!(result.len(), 3);
        // Should average pairs: (10+20)/2=15, (30+40)/2=35, (50+60)/2=55
        assert_eq!(result[0], 15);
        assert_eq!(result[1], 35);
        assert_eq!(result[2], 55);
    }

    #[test]
    fn test_render_sparkline_with_trend() {
        let values = vec![10, 20, 30, 40, 50];
        let result = render_sparkline_with_trend(&values, "↗️", Some(10));
        assert!(result.contains('▁'));
        assert!(result.contains("↗️"));
    }

    #[test]
    fn test_render_resource_with_sparkline() {
        let result = render_resource_with_sparkline(
            "250m",
            "1000m",
            25.0,
            &[10, 20, 30, 40, 50],
            "↗️",
        );
        assert!(result.contains("250m/1000m"));
        assert!(result.contains("[25%]"));
        assert!(result.contains("↗️"));
    }
}
