#[cfg(test)]
mod tests {
    use super::shared_ui::{SharedComponents, UiCache};
    use crate::tui::theme::{NaviTheme, ResourceStatus};
    
    #[test]
    fn test_centered_rect() {
        use ratatui::prelude::Rect;
        
        let area = Rect::new(0, 0, 100, 50);
        let centered = SharedComponents::centered_rect(50, 40, area);
        
        // Should be centered with 50% width and 40% height
        assert_eq!(centered.width, 50);
        assert_eq!(centered.height, 20);
        assert_eq!(centered.x, 25);
        assert_eq!(centered.y, 15);
    }
    
    #[test]
    fn test_health_bar_rendering() {
        let theme = NaviTheme::default();
        
        // Test 100% health
        let bar = SharedComponents::render_health_bar(100, 100, 20, &theme);
        assert!(bar.contains("100%"));
        assert!(bar.contains("🟩"));
        
        // Test 50% health
        let bar = SharedComponents::render_health_bar(50, 100, 20, &theme);
        assert!(bar.contains("50%"));
        assert!(bar.contains("🟧"));
        
        // Test 0% health
        let bar = SharedComponents::render_health_bar(0, 100, 20, &theme);
        assert!(bar.contains("0%"));
        assert!(bar.contains("🟥"));
    }
    
    #[test]
    fn test_status_indicator() {
        use ratatui::prelude::Color;
        let theme = NaviTheme::default();
        
        let (symbol, color) = SharedComponents::render_status_indicator(&ResourceStatus::Running, &theme);
        assert!(!symbol.is_empty());
        assert_eq!(color, theme.success);
        
        let (symbol, color) = SharedComponents::render_status_indicator(&ResourceStatus::Failed, &theme);
        assert!(!symbol.is_empty());
        assert_eq!(color, theme.error);
    }
    
    #[test]
    fn test_ui_cache_filter_functionality() {
        let mut cache = UiCache::default();
        let items = vec!["apple", "banana", "cherry", "date"];
        
        // Test initial filter
        cache.update_filter_cache("a", &items, |item, filter| item.contains(filter));
        let indices = cache.get_filtered_indices();
        assert_eq!(indices.len(), 3); // apple, banana, date
        assert_eq!(cache.last_filter, "a");
        
        // Test filter change
        cache.update_filter_cache("ch", &items, |item, filter| item.contains(filter));
        let indices = cache.get_filtered_indices();
        assert_eq!(indices.len(), 1); // cherry
        assert_eq!(cache.last_filter, "ch");
        
        // Test same filter (should not recompute)
        let old_indices = indices.clone();
        cache.update_filter_cache("ch", &items, |item, filter| item.contains(filter));
        let new_indices = cache.get_filtered_indices();
        assert_eq!(old_indices, new_indices);
    }
    
    #[test]
    fn test_ui_cache_stats_functionality() {
        let mut cache = UiCache::default();
        
        // Test initial stats
        cache.update_stats_cache("Initial stats".to_string(), 1);
        assert_eq!(cache.get_cached_stats(), Some(&"Initial stats".to_string()));
        assert_eq!(cache.stats_generation, 1);
        
        // Test stats update
        cache.update_stats_cache("Updated stats".to_string(), 2);
        assert_eq!(cache.get_cached_stats(), Some(&"Updated stats".to_string()));
        assert_eq!(cache.stats_generation, 2);
        
        // Test same generation (should not update)
        cache.update_stats_cache("Should not update".to_string(), 2);
        assert_eq!(cache.get_cached_stats(), Some(&"Updated stats".to_string()));
    }
    
    #[test]
    fn test_empty_filter_includes_all_items() {
        let mut cache = UiCache::default();
        let items = vec!["apple", "banana", "cherry"];
        
        cache.update_filter_cache("", &items, |item, filter| item.contains(filter));
        let indices = cache.get_filtered_indices();
        assert_eq!(indices.len(), 3);
        assert_eq!(indices, &[0, 1, 2]);
    }
}